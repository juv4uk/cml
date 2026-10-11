//! Compare the independent staged physical corpus against the SAME SENS
//! canonical reader in an isolated current-domain CML consumer.
//! This is a transport + admission witness, not a second compiler backend.
use cml_current_sens_transport::{VerifiedPhysicalProgram, modern_ir::lower_verified_program};
use std::{env, fs, path::Path};

fn failure(message: &str) -> ! {
    eprintln!("CML_MODERN_STAGE_BLOCKED: {message}");
    std::process::exit(1);
}

fn read_real_file(arg: &str, extension: &str) -> Vec<u8> {
    let path = Path::new(arg);
    if path.extension().and_then(|x| x.to_str()) != Some(extension) {
        failure("unexpected physical extension");
    }
    let meta = fs::symlink_metadata(path).unwrap_or_else(|_| failure("missing physical file"));
    if !meta.file_type().is_file() {
        failure("physical carrier must be a regular file, never a symlink");
    }
    fs::read(path).unwrap_or_else(|_| failure("cannot read physical carrier"))
}

fn trusted_digest(hex: &str) -> [u8; 32] {
    if hex.len() != 64 || !hex.bytes().all(|x| x.is_ascii_digit() || (b'a'..=b'f').contains(&x)) {
        failure("SHA-256 from pinned manifest must be 64 lowercase hex characters");
    }
    let mut digest = [0u8; 32];
    for i in 0..32 {
        digest[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .unwrap_or_else(|_| failure("invalid trusted manifest SHA-256"));
    }
    digest
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [mode, original] if mode == "--t5" => {
            let bytes = read_real_file(original, "sens");
            let admitted = VerifiedPhysicalProgram::from_t5(&bytes)
                .unwrap_or_else(|_| failure("SENS T5 exact canonical admission rejected"));
            if admitted.canonical_t5 != bytes {
                failure("T5 physical bytes changed");
            }
            println!("CML_MODERN_STAGE_PASS type=T5 bytes={}", bytes.len());
        }
        [mode, original, framed, hash] if mode == "--f3" => {
            let t5 = read_real_file(original, "sens");
            let f3 = read_real_file(framed, "senc");
            let oracle = VerifiedPhysicalProgram::from_t5(&t5)
                .unwrap_or_else(|_| failure("SENS T5 canonical oracle rejected"));
            let candidate = VerifiedPhysicalProgram::from_explicit_tiny_f3(
                &f3, trusted_digest(hash)
            ).unwrap_or_else(|_| failure("F3 carrier/hash/domain admission rejected"));
            if candidate.canonical_t5 != oracle.canonical_t5
                || candidate.exact_source != oracle.exact_source
                || f3.len() >= t5.len()
            {
                failure("F3 failed exact physical, source or byte saving comparison");
            }
            // The CML-owned typed AST/IR must be independent of physical codec.
            // Unsupported language forms must be rejected IDENTICALLY;
            // never interpret a non-callable D2 list as executable meaning.
            let a = lower_verified_program(&oracle);
            let b = lower_verified_program(&candidate);
            if a != b {
                failure("F3 and T5 disagree on verified modern AST/IR or rejection");
            }
            let ir_status = if a.is_ok() { "ADMITTED" } else { "UNSUPPORTED" };
            println!(
                "CML_MODERN_STAGE_PASS type=F3 t5_bytes={} f3_bytes={} ir={}",
                t5.len(), f3.len(), ir_status
            );
        }
        _ => failure("usage: verify_stage --t5 <path.sens> | --f3 <path.sens> <path.senc> <trusted_sha256>"),
    }
}
