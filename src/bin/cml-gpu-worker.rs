#![cfg(feature = "gpu-cuda")]

use cml::gpu_cuda_runtime::{discover_devices, execute_map};
use cml::ir::{BufferLiteral, Ir, Params};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"CMLG";
const VERSION: u8 = 1;
const OP_PING: u8 = 1;
const OP_PROBE: u8 = 2;
const OP_ADD_I32: u8 = 3;
const STATUS_OK: u8 = 0;
const STATUS_ERR: u8 = 1;

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

fn main() {
    if let Err(error) = run() {
        eprintln!("cml-gpu-worker: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("serve") => serve(socket_path()),
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
        _ => Err("usage: cml-gpu-worker serve|ping|probe|add-i32 <offset> <value>...".into()),
    }
}

fn serve(path: PathBuf) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(&path).map_err(|error| format!("remove stale socket {}: {error}", path.display()))?;
    }
    let listener = UnixListener::bind(&path)
        .map_err(|error| format!("bind {}: {error}", path.display()))?;
    let _guard = SocketGuard(path.clone());
    eprintln!("cml-gpu-worker listening on {}", path.display());

    // Force discovery once so a bad CUDA/WSL setup fails before the runner submits work.
    let devices = discover_devices().map_err(|error| format!("CUDA discovery failed: {error:?}"))?;
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
            let devices = discover_devices().map_err(|error| format!("CUDA probe failed: {error:?}"))?;
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
        other => Err(format!("unknown opcode {other}")),
    }
}

fn add_i32_ir(values: Vec<i32>, offset: i64) -> Ir {
    Ir::App {
        func: Box::new(Ir::Sid(sens::sens!(01011001))),
        args: vec![
            Ir::Lambda {
                params: Params::Fixed(vec!["X".to_string()]),
                body: Box::new(Ir::App {
                    func: Box::new(Ir::Sid(sens::sens!(00001100))),
                    args: vec![Ir::Var("X".to_string()), Ir::Int(offset)],
                }),
            },
            Ir::Buffer(BufferLiteral::I32(values)),
        ],
    }
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
    println!("{}", values.iter().map(i32::to_string).collect::<Vec<_>>().join(" "));
    Ok(())
}

fn transact(path: &Path, opcode: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut stream = UnixStream::connect(path)
        .map_err(|error| format!("connect {}: {error}", path.display()))?;
    write_request(&mut stream, opcode, payload).map_err(io_error)?;
    let (status, body) = read_response(&mut stream)?;
    if status == STATUS_OK {
        Ok(body)
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
        return Err(format!("unsupported GPU worker protocol version {}", header[4]));
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

fn decode_add_request(payload: &[u8]) -> Result<(i64, Vec<i32>), String> {
    if payload.len() < 12 {
        return Err("ADD_I32 payload too short".into());
    }
    let offset = i64::from_le_bytes(payload[..8].try_into().unwrap());
    let count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    let expected = 12usize.checked_add(count.checked_mul(4).ok_or("ADD_I32 count overflow")?)
        .ok_or("ADD_I32 length overflow")?;
    if payload.len() != expected {
        return Err(format!("ADD_I32 payload length mismatch: got {}, expected {expected}", payload.len()));
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
    let expected = 4usize.checked_add(count.checked_mul(4).ok_or("i32 count overflow")?)
        .ok_or("i32 response length overflow")?;
    if body.len() != expected {
        return Err(format!("i32 response length mismatch: got {}, expected {expected}", body.len()));
    }
    Ok(body[4..]
        .chunks_exact(4)
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

fn io_error(error: io::Error) -> String {
    error.to_string()
}
