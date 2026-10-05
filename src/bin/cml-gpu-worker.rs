#[cfg(not(feature = "gpu-cuda"))]
fn main() {
    eprintln!("cml-gpu-worker requires --features gpu-cuda");
    std::process::exit(2);
}

#[cfg(feature = "gpu-cuda")]
fn main() {
    enabled::main();
}

#[cfg(feature = "gpu-cuda")]
mod enabled {
    use cml::gpu_cuda_runtime::{discover_devices, execute_map, execute_map_chain_i32_selected};
    use cml::ir::{BufferLiteral, Ir, Params};
    use std::env;
    use std::fs;
    use std::io::{self, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    const MAGIC: &[u8; 4] = b"CMLG";
    const VERSION: u8 = 1;
    const OP_PING: u8 = 1;
    const OP_PROBE: u8 = 2;
    const OP_ADD_I32: u8 = 3;
    const OP_CHAIN_FILE_I32: u8 = 4;
    const OP_CHAIN_FILE_I32_PROVENANCE: u8 = 5;
    const STATUS_OK: u8 = 0;
    const STATUS_ERR: u8 = 1;
    const MAX_PROVENANCE_FIELD_BYTES: usize = 128;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ClientProvenance {
        repository: String,
        run_id: String,
        job: String,
        case_id: String,
    }

    #[derive(Debug)]
    struct TimedTransaction {
        body: Vec<u8>,
        client_round_trip_ns: u128,
    }

    fn socket_path() -> PathBuf {
        env::var_os("CML_GPU_WORKER_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/cml-gpu-worker.sock"))
    }

    struct SocketGuard(PathBuf);

    impl Drop for SocketGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    pub fn main() {
        if let Err(error) = run() {
            eprintln!("cml-gpu-worker: {error}");
            std::process::exit(1);
        }
    }

    fn run() -> Result<(), String> {
        let mut args = env::args().skip(1);
        match args.next().as_deref() {
            Some("serve") => {
                let mut reserve_bytes = None;
                while let Some(arg) = args.next() {
                    if arg == "--reserve-bytes" {
                        let bytes: usize = args
                            .next()
                            .ok_or("--reserve-bytes requires <bytes>")?
                            .parse()
                            .map_err(|error| format!("invalid reserve-bytes: {error}"))?;
                        reserve_bytes = Some(bytes);
                    } else {
                        return Err(format!("unknown serve argument: {arg}"));
                    }
                }
                if let Some(bytes) = reserve_bytes {
                    // Rust 2024 marks process-environment mutation unsafe because
                    // concurrent readers may exist. This happens before serve()
                    // starts threads or accepts clients.
                    unsafe {
                        env::set_var("CML_CUDA_MEMORY_RESERVE_BYTES", bytes.to_string());
                    }
                }
                serve(socket_path())
            }
            Some("ping") => client_ping(&socket_path()),
            Some("probe") => client_probe(&socket_path()),
            Some("add-i32") => {
                let offset: i64 = args
                    .next()
                    .ok_or("add-i32 requires <offset> <value>...")?
                    .parse()
                    .map_err(|error| format!("invalid offset: {error}"))?;
                let values = args
                    .map(|value| value.parse::<i32>().map_err(|error| format!("invalid i32 {value}: {error}")))
                    .collect::<Result<Vec<_>, _>>()?;
                if values.is_empty() {
                    return Err("add-i32 requires at least one value".into());
                }
                client_add_i32(&socket_path(), offset, &values)
            }
            Some("chain-file-i32") => {
                let input = args
                    .next()
                    .ok_or("chain-file-i32 requires <input.bin> <output.bin> <offset>...")?;
                let output = args
                    .next()
                    .ok_or("chain-file-i32 requires <input.bin> <output.bin> <offset>...")?;
                let offsets = args
                    .map(|value| value.parse::<i64>().map_err(|error| format!("invalid offset {value}: {error}")))
                    .collect::<Result<Vec<_>, _>>()?;
                if offsets.is_empty() {
                    return Err("chain-file-i32 requires at least one offset".into());
                }
                client_chain_file_i32(
                    &socket_path(),
                    &absolute_path(&input)?,
                    &absolute_path(&output)?,
                    &offsets,
                )
            }
            Some("chain-file-i32-provenance") => {
                let required = "chain-file-i32-provenance requires <repository> <run-id> <job> <case-id> <input.bin> <output.bin> <offset>...";
                let provenance = ClientProvenance {
                    repository: args.next().ok_or(required)?,
                    run_id: args.next().ok_or(required)?,
                    job: args.next().ok_or(required)?,
                    case_id: args.next().ok_or(required)?,
                };
                validate_provenance(&provenance)?;
                let input = args.next().ok_or(required)?;
                let output = args.next().ok_or(required)?;
                let offsets = args
                    .map(|value| value.parse::<i64>().map_err(|error| format!("invalid offset {value}: {error}")))
                    .collect::<Result<Vec<_>, _>>()?;
                if offsets.is_empty() {
                    return Err("chain-file-i32-provenance requires at least one offset".into());
                }
                client_chain_file_i32_provenance(
                    &socket_path(),
                    &provenance,
                    &absolute_path(&input)?,
                    &absolute_path(&output)?,
                    &offsets,
                )
            }
            _ => Err(
                "usage: cml-gpu-worker serve [--reserve-bytes <bytes>]|ping|probe|add-i32 <offset> <value>...|chain-file-i32 <input.bin> <output.bin> <offset>...|chain-file-i32-provenance <repository> <run-id> <job> <case-id> <input.bin> <output.bin> <offset>..."
                    .into(),
            ),
        }
    }

    fn serve(path: PathBuf) -> Result<(), String> {
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|error| format!("remove stale socket {}: {error}", path.display()))?;
        }
        let listener = UnixListener::bind(&path)
            .map_err(|error| format!("bind {}: {error}", path.display()))?;
        let _guard = SocketGuard(path.clone());
        eprintln!("cml-gpu-worker listening on {}", path.display());

        // Force discovery once so a bad CUDA/WSL setup fails before the runner submits work.
        let devices =
            discover_devices().map_err(|error| format!("CUDA discovery failed: {error:?}"))?;
        if devices.is_empty() {
            return Err("CUDA runtime reported zero devices".into());
        }
        eprintln!("cml-gpu-worker ready: {:?}", devices[0]);

        for connection in listener.incoming() {
            match connection {
                Ok(mut stream) => {
                    if let Err(error) = handle(&mut stream) {
                        let _ = write_response(&mut stream, STATUS_ERR, error.as_bytes());
                    }
                }
                Err(error) => eprintln!("accept failed: {error}"),
            }
        }
        Ok(())
    }

    fn handle(stream: &mut UnixStream) -> Result<(), String> {
        let (opcode, payload) = read_request(stream)?;
        match opcode {
            OP_PING => write_response(stream, STATUS_OK, b"pong").map_err(io_error),
            OP_PROBE => {
                let devices =
                    discover_devices().map_err(|error| format!("CUDA probe failed: {error:?}"))?;
                let text = format!("{devices:?}");
                write_response(stream, STATUS_OK, text.as_bytes()).map_err(io_error)
            }
            OP_ADD_I32 => {
                let (offset, values) = decode_add_request(&payload)?;
                let ir = add_i32_ir(values, offset);
                let execution = execute_map(&ir, 0)
                    .map_err(|error| format!("CUDA execution failed: {error:?}"))?;
                let BufferLiteral::I32(values) = execution.output else {
                    return Err("CUDA returned a non-i32 buffer".into());
                };
                let body = encode_i32_values(&values);
                write_response(stream, STATUS_OK, &body).map_err(io_error)
            }
            OP_CHAIN_FILE_I32 => {
                let service_started = Instant::now();
                let (input_path, output_path, offsets) = decode_chain_file_request(&payload)?;
                let evidence = execute_chain_file_i32(&input_path, &output_path, &offsets)?;
                let server_service_ns = service_started.elapsed().as_nanos();
                let body = format!("{evidence} server_service_ns={server_service_ns}");
                write_response(stream, STATUS_OK, body.as_bytes()).map_err(io_error)
            }
            OP_CHAIN_FILE_I32_PROVENANCE => {
                let service_started = Instant::now();
                let (provenance, input_path, output_path, offsets) =
                    decode_chain_file_provenance_request(&payload)?;
                let evidence = execute_chain_file_i32(&input_path, &output_path, &offsets)?;
                let server_service_ns = service_started.elapsed().as_nanos();
                let body = format!(
                    "repository={} run_id={} job={} case_id={} {} server_service_ns={}",
                    provenance.repository,
                    provenance.run_id,
                    provenance.job,
                    provenance.case_id,
                    evidence,
                    server_service_ns
                );
                write_response(stream, STATUS_OK, body.as_bytes()).map_err(io_error)
            }
            other => Err(format!("unknown opcode {other}")),
        }
    }

    fn execute_chain_file_i32(
        input_path: &Path,
        output_path: &Path,
        offsets: &[i64],
    ) -> Result<String, String> {
        let values = read_i32_file(input_path)?;
        let functions: Vec<Ir> = offsets.iter().copied().map(add_i32_function).collect();
        let input = BufferLiteral::I32(values);
        let cuda_started = Instant::now();
        let execution =
            execute_map_chain_i32_selected(&functions, &input, &[functions.len() - 1], 0)
                .map_err(|error| format!("CUDA chain execution failed: {error:?}"))?;
        let cuda_ns = cuda_started.elapsed().as_nanos();
        let Some((_, BufferLiteral::I32(output))) = execution.outputs.into_iter().next() else {
            return Err("CUDA chain returned no final i32 buffer".into());
        };
        write_i32_file(output_path, &output)?;
        Ok(format!(
            "count={} steps={} cuda_ns={} output={}",
            output.len(),
            offsets.len(),
            cuda_ns,
            output_path.display()
        ))
    }

    fn add_i32_function(offset: i64) -> Ir {
        Ir::Lambda {
            params: Params::Fixed(vec!["X".to_string()]),
            body: Box::new(Ir::App {
                func: Box::new(Ir::Sid(sens::sens!(00001100))),
                args: vec![Ir::Var("X".to_string()), Ir::Int(offset)],
            }),
        }
    }

    fn add_i32_ir(values: Vec<i32>, offset: i64) -> Ir {
        Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(01011001))),
            args: vec![
                add_i32_function(offset),
                Ir::Buffer(BufferLiteral::I32(values)),
            ],
        }
    }

    fn absolute_path(value: &str) -> Result<PathBuf, String> {
        let path = PathBuf::from(value);
        if path.is_absolute() {
            Ok(path)
        } else {
            env::current_dir()
                .map(|dir| dir.join(path))
                .map_err(|error| format!("resolve path {value}: {error}"))
        }
    }

    fn read_i32_file(path: &Path) -> Result<Vec<i32>, String> {
        let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
        if bytes.is_empty() || bytes.len() % 4 != 0 {
            return Err(format!(
                "i32 input {} must be non-empty and a multiple of 4 bytes",
                path.display()
            ));
        }
        Ok(bytes
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
            .collect())
    }

    fn write_i32_file(path: &Path, values: &[i32]) -> Result<(), String> {
        let mut bytes = Vec::with_capacity(values.len() * 4);
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
    }

    fn client_ping(path: &Path) -> Result<(), String> {
        let body = transact(path, OP_PING, &[])?;
        println!("{}", String::from_utf8_lossy(&body));
        Ok(())
    }

    fn client_probe(path: &Path) -> Result<(), String> {
        let body = transact(path, OP_PROBE, &[])?;
        println!("{}", String::from_utf8_lossy(&body));
        Ok(())
    }

    fn client_add_i32(path: &Path, offset: i64, values: &[i32]) -> Result<(), String> {
        let mut payload = Vec::with_capacity(12 + values.len() * 4);
        payload.extend_from_slice(&offset.to_le_bytes());
        payload.extend_from_slice(&(values.len() as u32).to_le_bytes());
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let body = transact(path, OP_ADD_I32, &payload)?;
        let values = decode_i32_values(&body)?;
        println!(
            "{}",
            values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        );
        Ok(())
    }

    fn client_chain_file_i32(
        socket: &Path,
        input: &Path,
        output: &Path,
        offsets: &[i64],
    ) -> Result<(), String> {
        let payload = encode_chain_file_payload(input, output, offsets)?;
        let transaction = transact_timed(socket, OP_CHAIN_FILE_I32, &payload)?;
        print_timed_chain_evidence(transaction)
    }

    fn client_chain_file_i32_provenance(
        socket: &Path,
        provenance: &ClientProvenance,
        input: &Path,
        output: &Path,
        offsets: &[i64],
    ) -> Result<(), String> {
        let mut payload = Vec::new();
        encode_provenance(&mut payload, provenance)?;
        payload.extend_from_slice(&encode_chain_file_payload(input, output, offsets)?);
        let transaction = transact_timed(socket, OP_CHAIN_FILE_I32_PROVENANCE, &payload)?;
        print_timed_chain_evidence(transaction)
    }

    fn print_timed_chain_evidence(transaction: TimedTransaction) -> Result<(), String> {
        let body = String::from_utf8(transaction.body)
            .map_err(|error| format!("GPU worker timing response is not UTF-8: {error}"))?;
        let server_service_ns = metric_u128(&body, "server_service_ns")?;
        let wait_protocol_ns =
            wait_protocol_residual_ns(transaction.client_round_trip_ns, server_service_ns);
        println!(
            "{body} client_round_trip_ns={} wait_protocol_ns={}",
            transaction.client_round_trip_ns, wait_protocol_ns
        );
        Ok(())
    }

    /// Residual between client-observed round-trip and server service time.
    ///
    /// This is deliberately named wait/protocol overhead, not pure queue wait:
    /// it includes socket connect/framing/copy costs plus any time waiting for
    /// the synchronous worker to accept and handle this request.
    fn wait_protocol_residual_ns(client_round_trip_ns: u128, server_service_ns: u128) -> u128 {
        client_round_trip_ns.saturating_sub(server_service_ns)
    }

    fn metric_u128(text: &str, name: &str) -> Result<u128, String> {
        let prefix = format!("{name}=");
        let value = text
            .split_whitespace()
            .find_map(|field| field.strip_prefix(&prefix))
            .ok_or_else(|| format!("GPU worker response missing {name}"))?;
        value
            .parse::<u128>()
            .map_err(|error| format!("invalid {name} value {value}: {error}"))
    }

    fn encode_chain_file_payload(
        input: &Path,
        output: &Path,
        offsets: &[i64],
    ) -> Result<Vec<u8>, String> {
        let mut payload = Vec::new();
        encode_path(&mut payload, input)?;
        encode_path(&mut payload, output)?;
        let count = u32::try_from(offsets.len()).map_err(|_| "too many chain offsets")?;
        payload.extend_from_slice(&count.to_le_bytes());
        for offset in offsets {
            payload.extend_from_slice(&offset.to_le_bytes());
        }
        Ok(payload)
    }

    fn transact(path: &Path, opcode: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
        Ok(transact_timed(path, opcode, payload)?.body)
    }

    fn transact_timed(
        path: &Path,
        opcode: u8,
        payload: &[u8],
    ) -> Result<TimedTransaction, String> {
        let round_trip_started = Instant::now();
        let mut stream = UnixStream::connect(path)
            .map_err(|error| format!("connect {}: {error}", path.display()))?;
        write_request(&mut stream, opcode, payload).map_err(io_error)?;
        let (status, body) = read_response(&mut stream)?;
        let client_round_trip_ns = round_trip_started.elapsed().as_nanos();
        if status == STATUS_OK {
            Ok(TimedTransaction {
                body,
                client_round_trip_ns,
            })
        } else {
            Err(String::from_utf8_lossy(&body).into_owned())
        }
    }

    fn write_request(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> io::Result<()> {
        stream.write_all(MAGIC)?;
        stream.write_all(&[VERSION, opcode])?;
        stream.write_all(&(payload.len() as u32).to_le_bytes())?;
        stream.write_all(payload)
    }

    fn read_request(stream: &mut UnixStream) -> Result<(u8, Vec<u8>), String> {
        let (kind, body) = read_frame(stream)?;
        Ok((kind, body))
    }

    fn write_response(stream: &mut UnixStream, status: u8, payload: &[u8]) -> io::Result<()> {
        stream.write_all(MAGIC)?;
        stream.write_all(&[VERSION, status])?;
        stream.write_all(&(payload.len() as u32).to_le_bytes())?;
        stream.write_all(payload)
    }

    fn read_response(stream: &mut UnixStream) -> Result<(u8, Vec<u8>), String> {
        read_frame(stream)
    }

    fn read_frame(stream: &mut UnixStream) -> Result<(u8, Vec<u8>), String> {
        let mut header = [0u8; 10];
        stream.read_exact(&mut header).map_err(io_error)?;
        if &header[..4] != MAGIC {
            return Err("bad GPU worker frame magic".into());
        }
        if header[4] != VERSION {
            return Err(format!(
                "unsupported GPU worker protocol version {}",
                header[4]
            ));
        }
        let kind = header[5];
        let len = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
        if len > 64 * 1024 * 1024 {
            return Err("GPU worker frame exceeds 64 MiB".into());
        }
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).map_err(io_error)?;
        Ok((kind, body))
    }

    fn validate_provenance(provenance: &ClientProvenance) -> Result<(), String> {
        validate_provenance_field("repository", &provenance.repository)?;
        validate_provenance_field("run_id", &provenance.run_id)?;
        validate_provenance_field("job", &provenance.job)?;
        validate_provenance_field("case_id", &provenance.case_id)
    }

    fn validate_provenance_field(name: &str, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err(format!("provenance {name} must not be empty"));
        }
        if value.len() > MAX_PROVENANCE_FIELD_BYTES {
            return Err(format!(
                "provenance {name} exceeds {MAX_PROVENANCE_FIELD_BYTES} bytes"
            ));
        }
        if !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
        }) {
            return Err(format!(
                "provenance {name} must use only ASCII alphanumeric or . _ : / -"
            ));
        }
        Ok(())
    }

    fn encode_provenance(
        payload: &mut Vec<u8>,
        provenance: &ClientProvenance,
    ) -> Result<(), String> {
        validate_provenance(provenance)?;
        for value in [
            &provenance.repository,
            &provenance.run_id,
            &provenance.job,
            &provenance.case_id,
        ] {
            let bytes = value.as_bytes();
            let len = u16::try_from(bytes.len()).map_err(|_| "provenance field too long")?;
            payload.extend_from_slice(&len.to_le_bytes());
            payload.extend_from_slice(bytes);
        }
        Ok(())
    }

    fn decode_provenance(payload: &[u8], cursor: &mut usize) -> Result<ClientProvenance, String> {
        let provenance = ClientProvenance {
            repository: decode_provenance_field(payload, cursor, "repository")?,
            run_id: decode_provenance_field(payload, cursor, "run_id")?,
            job: decode_provenance_field(payload, cursor, "job")?,
            case_id: decode_provenance_field(payload, cursor, "case_id")?,
        };
        validate_provenance(&provenance)?;
        Ok(provenance)
    }

    fn decode_provenance_field(
        payload: &[u8],
        cursor: &mut usize,
        name: &str,
    ) -> Result<String, String> {
        if payload.len().saturating_sub(*cursor) < 2 {
            return Err(format!("provenance {name} length missing"));
        }
        let len = u16::from_le_bytes(payload[*cursor..*cursor + 2].try_into().unwrap()) as usize;
        *cursor += 2;
        if len > MAX_PROVENANCE_FIELD_BYTES {
            return Err(format!(
                "provenance {name} exceeds {MAX_PROVENANCE_FIELD_BYTES} bytes"
            ));
        }
        let end = cursor
            .checked_add(len)
            .ok_or_else(|| format!("provenance {name} length overflow"))?;
        if end > payload.len() {
            return Err(format!("provenance {name} payload truncated"));
        }
        let value = std::str::from_utf8(&payload[*cursor..end])
            .map_err(|error| format!("provenance {name} is not UTF-8: {error}"))?
            .to_string();
        *cursor = end;
        Ok(value)
    }

    fn encode_path(payload: &mut Vec<u8>, path: &Path) -> Result<(), String> {
        let text = path
            .to_str()
            .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))?;
        let bytes = text.as_bytes();
        let len = u32::try_from(bytes.len()).map_err(|_| "path too long")?;
        payload.extend_from_slice(&len.to_le_bytes());
        payload.extend_from_slice(bytes);
        Ok(())
    }

    fn decode_path(payload: &[u8], cursor: &mut usize) -> Result<PathBuf, String> {
        if payload.len().saturating_sub(*cursor) < 4 {
            return Err("path length missing".into());
        }
        let len = u32::from_le_bytes(payload[*cursor..*cursor + 4].try_into().unwrap()) as usize;
        *cursor += 4;
        let end = (*cursor).checked_add(len).ok_or("path length overflow")?;
        if end > payload.len() {
            return Err("path payload truncated".into());
        }
        let text = std::str::from_utf8(&payload[*cursor..end])
            .map_err(|error| format!("path is not UTF-8: {error}"))?;
        *cursor = end;
        Ok(PathBuf::from(text))
    }

    fn decode_chain_file_request(payload: &[u8]) -> Result<(PathBuf, PathBuf, Vec<i64>), String> {
        let mut cursor = 0usize;
        decode_chain_file_request_from(payload, &mut cursor)
    }

    fn decode_chain_file_provenance_request(
        payload: &[u8],
    ) -> Result<(ClientProvenance, PathBuf, PathBuf, Vec<i64>), String> {
        let mut cursor = 0usize;
        let provenance = decode_provenance(payload, &mut cursor)?;
        let (input, output, offsets) = decode_chain_file_request_from(payload, &mut cursor)?;
        Ok((provenance, input, output, offsets))
    }

    fn decode_chain_file_request_from(
        payload: &[u8],
        cursor: &mut usize,
    ) -> Result<(PathBuf, PathBuf, Vec<i64>), String> {
        let input = decode_path(payload, cursor)?;
        let output = decode_path(payload, cursor)?;
        if payload.len().saturating_sub(*cursor) < 4 {
            return Err("chain offset count missing".into());
        }
        let count = u32::from_le_bytes(payload[*cursor..*cursor + 4].try_into().unwrap()) as usize;
        *cursor += 4;
        if count == 0 {
            return Err("chain requires at least one offset".into());
        }
        let bytes = count.checked_mul(8).ok_or("chain offset count overflow")?;
        let end = cursor.checked_add(bytes).ok_or("chain payload overflow")?;
        if end != payload.len() {
            return Err(format!(
                "chain payload length mismatch: got {}, expected {end}",
                payload.len()
            ));
        }
        let offsets = payload[*cursor..end]
            .chunks_exact(8)
            .map(|chunk| i64::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        *cursor = end;
        Ok((input, output, offsets))
    }

    fn decode_add_request(payload: &[u8]) -> Result<(i64, Vec<i32>), String> {
        if payload.len() < 12 {
            return Err("ADD_I32 payload too short".into());
        }
        let offset = i64::from_le_bytes(payload[..8].try_into().unwrap());
        let count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
        let expected = 12usize
            .checked_add(count.checked_mul(4).ok_or("ADD_I32 count overflow")?)
            .ok_or("ADD_I32 length overflow")?;
        if payload.len() != expected {
            return Err(format!(
                "ADD_I32 payload length mismatch: got {}, expected {expected}",
                payload.len()
            ));
        }
        let mut values = Vec::with_capacity(count);
        for chunk in payload[12..].chunks_exact(4) {
            values.push(i32::from_le_bytes(chunk.try_into().unwrap()));
        }
        Ok((offset, values))
    }

    fn encode_i32_values(values: &[i32]) -> Vec<u8> {
        let mut body = Vec::with_capacity(4 + values.len() * 4);
        body.extend_from_slice(&(values.len() as u32).to_le_bytes());
        for value in values {
            body.extend_from_slice(&value.to_le_bytes());
        }
        body
    }

    fn decode_i32_values(body: &[u8]) -> Result<Vec<i32>, String> {
        if body.len() < 4 {
            return Err("i32 response too short".into());
        }
        let count = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
        let expected = 4usize
            .checked_add(count.checked_mul(4).ok_or("i32 count overflow")?)
            .ok_or("i32 response length overflow")?;
        if body.len() != expected {
            return Err(format!(
                "i32 response length mismatch: got {}, expected {expected}",
                body.len()
            ));
        }
        Ok(body[4..]
            .chunks_exact(4)
            .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
            .collect())
    }

    fn io_error(error: io::Error) -> String {
        error.to_string()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sample_provenance() -> ClientProvenance {
            ClientProvenance {
                repository: "juv4uk/sens".into(),
                run_id: "37307131226".into(),
                job: "law-miner-gpu".into(),
                case_id: "d6-selector-negative-001".into(),
            }
        }

        #[test]
        fn provenance_codec_round_trips() {
            let expected = sample_provenance();
            let mut payload = Vec::new();
            encode_provenance(&mut payload, &expected).expect("encode provenance");
            let mut cursor = 0;
            let decoded = decode_provenance(&payload, &mut cursor).expect("decode provenance");
            assert_eq!(decoded, expected);
            assert_eq!(cursor, payload.len());
        }

        #[test]
        fn provenance_rejects_empty_oversize_and_unsafe_characters() {
            let mut provenance = sample_provenance();
            provenance.job.clear();
            assert!(validate_provenance(&provenance).is_err());

            provenance = sample_provenance();
            provenance.case_id = "x".repeat(MAX_PROVENANCE_FIELD_BYTES + 1);
            assert!(validate_provenance(&provenance).is_err());

            provenance = sample_provenance();
            provenance.job = "job with spaces".into();
            assert!(validate_provenance(&provenance).is_err());

            provenance = sample_provenance();
            provenance.case_id = "case\nnext".into();
            assert!(validate_provenance(&provenance).is_err());
        }

        #[test]
        fn provenance_decoder_rejects_truncated_and_invalid_utf8() {
            let mut payload = Vec::new();
            payload.extend_from_slice(&4u16.to_le_bytes());
            payload.extend_from_slice(b"abc");
            let mut cursor = 0;
            assert!(decode_provenance_field(&payload, &mut cursor, "repository").is_err());

            let payload = [1u8, 0, 0xff];
            let mut cursor = 0;
            assert!(decode_provenance_field(&payload, &mut cursor, "repository").is_err());
        }

        #[test]
        fn provenance_chain_payload_round_trips_and_rejects_trailing_bytes() {
            let expected = sample_provenance();
            let input = Path::new("/tmp/input.i32");
            let output = Path::new("/tmp/output.i32");
            let offsets = [1, -2, 7];

            let mut payload = Vec::new();
            encode_provenance(&mut payload, &expected).expect("encode provenance");
            payload.extend_from_slice(
                &encode_chain_file_payload(input, output, &offsets).expect("encode chain"),
            );

            let (decoded, decoded_input, decoded_output, decoded_offsets) =
                decode_chain_file_provenance_request(&payload).expect("decode request");
            assert_eq!(decoded, expected);
            assert_eq!(decoded_input, input);
            assert_eq!(decoded_output, output);
            assert_eq!(decoded_offsets, offsets);

            payload.push(0);
            assert!(decode_chain_file_provenance_request(&payload).is_err());
        }

        #[test]
        fn wait_protocol_residual_is_explicit_and_saturating() {
            assert_eq!(wait_protocol_residual_ns(1_000, 600), 400);
            assert_eq!(wait_protocol_residual_ns(600, 1_000), 0);
        }

        #[test]
        fn timing_metric_parser_reads_named_field() {
            let evidence =
                "count=4 steps=2 cuda_ns=17 output=/tmp/out server_service_ns=9001";
            assert_eq!(metric_u128(evidence, "server_service_ns").unwrap(), 9001);
            assert!(metric_u128(evidence, "missing").is_err());
        }

        #[test]
        fn transact_timed_uses_monotonic_round_trip_measurement() {
            let socket = std::env::temp_dir().join(format!(
                "cml-gpu-worker-timing-{}.sock",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&socket);
            let listener = UnixListener::bind(&socket).expect("bind timing socket");
            let server_socket = socket.clone();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept timing client");
                let (opcode, payload) = read_request(&mut stream).expect("read timing request");
                assert_eq!(opcode, OP_PING);
                assert!(payload.is_empty());
                std::thread::sleep(std::time::Duration::from_millis(5));
                write_response(&mut stream, STATUS_OK, b"pong").expect("write timing response");
                drop(stream);
                let _ = std::fs::remove_file(server_socket);
            });

            let transaction =
                transact_timed(&socket, OP_PING, &[]).expect("timed local transaction");
            server.join().expect("timing server");
            assert_eq!(transaction.body, b"pong");
            assert!(
                transaction.client_round_trip_ns >= 1_000_000,
                "5ms artificial server delay must be observable in client round-trip"
            );
        }
    }
}
