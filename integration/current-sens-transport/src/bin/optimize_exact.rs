//! Opt-in SENS-specific candidate compiler driver (IR analysis only).
//! It never writes files, never runs a native CML backend, and does not infer
//! file type from header bytes. F3 requires an independent trusted digest.
use std::{env, fs, path::Path};
use cml_current_sens_transport::{
    VerifiedPhysicalProgram,
    modern_ir::lower_verified_program,
    current_exact_optimizer::{
        optimize_exact_ir, replay_optimizer_witness, replay_optimizer_witness_from_t5,
    },
};

fn checked_file(path: &str, extension: &str) -> Result<Vec<u8>, String> {
    let path = Path::new(path);
    if path.extension().and_then(|part| part.to_str()) != Some(extension) {
        return Err(format!("requires explicit .{extension} file extension"));
    }
    if !fs::symlink_metadata(path).map_err(|e| format!("file metadata: {e}"))?
        .file_type().is_file()
    {
        return Err("refuse symlink or other nonregular source".into());
    }
    fs::read(path).map_err(|e| format!("cannot read source: {e}"))
}

fn strict_sha256(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64
        || !hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("trusted SHA-256 must have exactly 64 lowercase hex digits".into());
    }
    let mut out = [0u8; 32];
    for (i, value) in out.iter_mut().enumerate() {
        *value = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| "trusted SHA-256 is invalid")?;
    }
    Ok(out)
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let (carrier, input) = match arguments.as_slice() {
        [mode, filename] if mode == "--t5" => {
            let physical = checked_file(filename, "sens")?;
            let verified = VerifiedPhysicalProgram::from_t5(&physical)
                .map_err(|e| format!("canonical T5 rejected: {e:?}"))?;
            ("T5", verified)
        }
        [mode, filename, trusted] if mode == "--f3" => {
            let physical = checked_file(filename, "senc")?;
            let hash = strict_sha256(trusted)?;
            let verified = VerifiedPhysicalProgram::from_explicit_tiny_f3(&physical, hash)
                .map_err(|e| format!("trusted physical F3 rejected: {e:?}"))?;
            ("F3", verified)
        }
        _ => return Err(
            "usage: optimize_exact --t5 SOURCE.sens | --f3 SOURCE.senc TRUSTED_SHA256".into()
        ),
    };
    let verified = lower_verified_program(&input)
        .map_err(|e| format!("SENS compiler-law AST/IR rejected: {e:?}"))?;
    let output = optimize_exact_ir(&verified)
        .map_err(|e| format!("domain-law rewrite rejected: {e:?}"))?;
    if !replay_optimizer_witness(&verified, &output)
        .map_err(|e| format!("rewrite replay rejected: {e:?}"))?
    {
        return Err("candidate IR rewrite witness did not replay".into());
    }
    if !replay_optimizer_witness_from_t5(&input.canonical_t5, &output)
        .map_err(|e| format!("original physical T5 replay rejected: {e:?}"))?
    {
        return Err("optimization witness does not belong to this physical T5".into());
    }
    let source_hash = output.canonical_t5_sha256.iter()
        .map(|b| format!("{b:02x}")).collect::<String>();
    println!(
        "SENS_OPT_O1_CANDIDATE_PASS carrier={} source_t5_sha256={} nodes_before={} nodes_after={} rewrites={} native_backend=NOT_VALIDATED",
        carrier, source_hash, output.original_nodes, output.optimized_nodes,
        output.witnesses.len()
    );
    for step in &output.witnesses {
        println!(
            "O1_WITNESS {:?} selector=D{}:{:0width$b} before={} after={} contract_sha256={}",
            step.law, step.selector_identity.width(), step.selector_identity.packed_bits(),
            step.before_nodes, step.after_nodes, step.authority_sha256,
            width = step.selector_identity.width(),
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("SENS_OPT_O1_BLOCKED: {error}");
        std::process::exit(1);
    }
}
