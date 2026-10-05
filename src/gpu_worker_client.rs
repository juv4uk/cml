//! Client-side adapter for the persistent local CUDA worker.
//!
//! This module is mechanism-only infrastructure. It accepts only the worker's
//! bounded protocol and never exposes arbitrary shell execution.

use crate::compute::{ScalarExpr, analyze};
use crate::execution::{ConcurrencyProfile, NodeExecutor};
use crate::ir::{BufferLiteral, Ir};
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"CMLG";
const VERSION: u8 = 1;
const OP_ADD_I32: u8 = 3;
const STATUS_OK: u8 = 0;
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Resolve the persistent worker socket using the same precedence for services,
/// CI runners, and interactive shells.
pub fn gpu_worker_socket_path() -> PathBuf {
    if let Some(path) = env::var_os("CML_GPU_WORKER_SOCKET") {
        return PathBuf::from(path);
    }
    if let Some(runtime_dir) = env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("cml-gpu-worker.sock");
    }
    PathBuf::from("/tmp/cml-gpu-worker.sock")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedCudaWorkerNodeExecutor {
    socket: PathBuf,
}

impl Default for SharedCudaWorkerNodeExecutor {
    fn default() -> Self {
        Self::new(gpu_worker_socket_path())
    }
}

impl SharedCudaWorkerNodeExecutor {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

impl NodeExecutor for SharedCudaWorkerNodeExecutor {
    fn execute_map(&self, ir: &Ir) -> Result<BufferLiteral, String> {
        let (offset, values) = admitted_add_i32(ir)?;
        let payload = encode_add_request(offset, &values)?;
        let body = transact(&self.socket, OP_ADD_I32, &payload)?;
        Ok(BufferLiteral::I32(decode_i32_values(&body)?))
    }

    fn concurrency_profile(&self) -> ConcurrencyProfile {
        ConcurrencyProfile::serial(format!("shared-cuda-worker:{}", self.socket.display()))
    }
}

fn admitted_add_i32(ir: &Ir) -> Result<(i64, Vec<i32>), String> {
    let analysis = analyze(ir);
    if !analysis.gpu_eligible() {
        return Err(format!(
            "shared CUDA worker refuses non-admitted region: {:?}",
            analysis.gpu_blockers
        ));
    }
    let region = analysis
        .region
        .ok_or_else(|| "shared CUDA worker requires an admitted compute region".to_string())?;
    let kernel = region
        .kernel
        .ok_or_else(|| "shared CUDA worker requires a lowered scalar kernel".to_string())?;
    let offset = match kernel.body {
        ScalarExpr::CheckedAdd(left, right) => match (*left, *right) {
            (ScalarExpr::Parameter(0), ScalarExpr::ExactInteger(value))
            | (ScalarExpr::ExactInteger(value), ScalarExpr::Parameter(0)) => value,
            _ => {
                return Err(
                    "shared CUDA worker protocol v1 admits only i32 parameter + constant".into(),
                );
            }
        },
        _ => {
            return Err(
                "shared CUDA worker protocol v1 admits only i32 checked-add kernels".into(),
            );
        }
    };
    let BufferLiteral::I32(values) = region.input else {
        return Err("shared CUDA worker protocol v1 admits only i32 buffers".into());
    };
    if values.is_empty() {
        return Err("shared CUDA worker refuses an empty i32 buffer".into());
    }
    Ok((offset, values))
}

fn encode_add_request(offset: i64, values: &[i32]) -> Result<Vec<u8>, String> {
    let count = u32::try_from(values.len()).map_err(|_| "too many i32 values")?;
    let mut payload = Vec::with_capacity(12 + values.len() * 4);
    payload.extend_from_slice(&offset.to_le_bytes());
    payload.extend_from_slice(&count.to_le_bytes());
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    Ok(payload)
}

fn transact(path: &Path, opcode: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut stream =
        UnixStream::connect(path).map_err(|error| format!("connect {}: {error}", path.display()))?;
    write_request(&mut stream, opcode, payload).map_err(|error| error.to_string())?;
    let (status, body) = read_frame(&mut stream)?;
    if status == STATUS_OK {
        Ok(body)
    } else {
        Err(String::from_utf8_lossy(&body).into_owned())
    }
}

fn write_request(stream: &mut UnixStream, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    stream.write_all(MAGIC)?;
    stream.write_all(&[VERSION, opcode])?;
    stream.write_all(&(payload.len() as u32).to_le_bytes())?;
    stream.write_all(payload)
}

fn read_frame(stream: &mut UnixStream) -> Result<(u8, Vec<u8>), String> {
    let mut header = [0u8; 10];
    stream.read_exact(&mut header).map_err(|error| error.to_string())?;
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
    if len > MAX_FRAME_BYTES {
        return Err("GPU worker frame exceeds 64 MiB".into());
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).map_err(|error| error.to_string())?;
    Ok((kind, body))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Params;
    use std::fs;
    use std::os::unix::net::UnixListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn add_map(values: Vec<i32>, offset: i64) -> Ir {
        Ir::App {
            func: Box::new(Ir::Sid(sens::sens!(01011001))),
            args: vec![
                Ir::Lambda {
                    params: Params::Fixed(vec!["X".into()]),
                    body: Box::new(Ir::App {
                        func: Box::new(Ir::Sid(sens::sens!(00001100))),
                        args: vec![Ir::Var("X".into()), Ir::Int(offset)],
                    }),
                },
                Ir::Buffer(BufferLiteral::I32(values)),
            ],
        }
    }

    fn temp_socket() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("cml-shared-worker-test-{}-{nonce}.sock", std::process::id()))
    }

    #[test]
    fn shared_worker_adapter_uses_bounded_add_i32_protocol() {
        let socket = temp_socket();
        let listener = UnixListener::bind(&socket).unwrap();
        let server_socket = socket.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = [0u8; 10];
            stream.read_exact(&mut header).unwrap();
            assert_eq!(&header[..4], MAGIC);
            assert_eq!(header[4], VERSION);
            assert_eq!(header[5], OP_ADD_I32);
            let len = u32::from_le_bytes(header[6..10].try_into().unwrap()) as usize;
            let mut payload = vec![0u8; len];
            stream.read_exact(&mut payload).unwrap();

            let offset = i64::from_le_bytes(payload[..8].try_into().unwrap());
            let count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
            assert_eq!(offset, 7);
            assert_eq!(count, 4);
            let input: Vec<i32> = payload[12..]
                .chunks_exact(4)
                .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();
            assert_eq!(input, vec![1, 2, 3, 4]);

            let output = vec![8i32, 9, 10, 11];
            let mut body = Vec::new();
            body.extend_from_slice(&(output.len() as u32).to_le_bytes());
            for value in output {
                body.extend_from_slice(&value.to_le_bytes());
            }
            stream.write_all(MAGIC).unwrap();
            stream.write_all(&[VERSION, STATUS_OK]).unwrap();
            stream
                .write_all(&(body.len() as u32).to_le_bytes())
                .unwrap();
            stream.write_all(&body).unwrap();
            drop(stream);
            drop(listener);
            let _ = fs::remove_file(server_socket);
        });

        let executor = SharedCudaWorkerNodeExecutor::new(&socket);
        let result = executor.execute_map(&add_map(vec![1, 2, 3, 4], 7)).unwrap();
        assert_eq!(result, BufferLiteral::I32(vec![8, 9, 10, 11]));
        server.join().unwrap();
    }
}
