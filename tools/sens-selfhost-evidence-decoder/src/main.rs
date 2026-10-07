use sens::Value;
use std::io::{Read, Write};

const HEADER: &str = "sens-c1-evidence-decoded/1";

fn list_values(value: &Value) -> Result<Vec<&Value>, String> {
    let mut out = Vec::new();
    let mut cursor = value;
    loop {
        match cursor {
            Value::Nil => return Ok(out),
            Value::Pair(head, tail) => {
                out.push(head.as_ref());
                cursor = tail.as_ref();
            }
            other => return Err(format!("expected proper list, got {other}")),
        }
    }
}

fn symbol(value: &Value, label: &str) -> Result<String, String> {
    match value {
        Value::Symbol(text) => Ok(text.to_string()),
        other => Err(format!("{label} must be a symbol, got {other}")),
    }
}

fn string(value: &Value, label: &str) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.to_string()),
        other => Err(format!("{label} must be a string, got {other}")),
    }
}

fn field<'a>(artifact: &'a Value, name: &str) -> Result<&'a Value, String> {
    let rows = list_values(artifact)?;
    for row in rows.iter().skip(1) {
        let parts = list_values(row)?;
        if parts.len() != 2 {
            return Err(format!("artifact field must have two items, got {}", parts.len()));
        }
        if matches!(parts[0], Value::Symbol(found) if found.as_ref() == name) {
            return Ok(parts[1]);
        }
    }
    Err(format!("missing artifact field {name}"))
}

fn token(value: &str, label: &str) -> Result<&str, String> {
    if value.is_empty() || value.bytes().any(|b| matches!(b, b'\t' | b'\n' | b'\r')) {
        return Err(format!("{label} is not a safe TSV token"));
    }
    Ok(value)
}

fn meta(out: &mut impl Write, name: &str, value: &str) -> Result<(), String> {
    writeln!(out, "meta\t{}\t{}", token(name, "meta name")?, token(value, name)?)
        .map_err(|e| e.to_string())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("sens-selfhost-evidence-decoder: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read stdin: {e}"))?;

    let artifact = sens::compiler_evidence_from_canonical_bytes(&bytes)?;
    let reencoded = sens::compiler_evidence_canonical_bytes(&artifact)?;
    if reencoded != bytes {
        return Err("decoded compiler evidence does not round-trip byte-for-byte".into());
    }

    let rows = list_values(&artifact)?;
    if rows.is_empty()
        || !matches!(rows[0], Value::Symbol(name) if name.as_ref() == "compiler-compilation-artifact/1")
    {
        return Err("expected compiler-compilation-artifact/1".into());
    }
    if symbol(field(&artifact, "artifact-kind")?, "artifact-kind")? != "whole-program" {
        return Err("artifact-kind must be whole-program".into());
    }
    if !matches!(field(&artifact, "required-capabilities")?, Value::Nil) {
        return Err("selfhost whole-program artifact must require no capabilities".into());
    }
    if symbol(field(&artifact, "artifact-status")?, "artifact-status")?
        != "canonical-backend-neutral"
    {
        return Err("artifact-status must be canonical-backend-neutral".into());
    }

    let authority = list_values(field(&artifact, "authority-provenance")?)?;
    if authority.len() != 5 {
        return Err(format!(
            "authority-provenance must contain five strings, got {}",
            authority.len()
        ));
    }
    let revision = string(authority[0], "authority revision")?;
    let authority_path = string(authority[1], "authority path")?;
    let authority_sha256 = string(authority[2], "authority sha256")?;
    let contract = string(authority[3], "contract version")?;
    let nucleus_sha256 = string(authority[4], "compiler nucleus sha256")?;

    let requests = list_values(field(&artifact, "semantic-requests")?)?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "{HEADER}").map_err(|e| e.to_string())?;
    meta(
        &mut out,
        "program-wire-sha256",
        &string(field(&artifact, "program-wire-sha256")?, "program wire sha256")?,
    )?;
    meta(
        &mut out,
        "semantic-requests-sha256",
        &string(
            field(&artifact, "semantic-requests-sha256")?,
            "semantic requests sha256",
        )?,
    )?;
    meta(&mut out, "sens-revision", &revision)?;
    meta(&mut out, "authority-path", &authority_path)?;
    meta(&mut out, "authority-sha256", &authority_sha256)?;
    meta(&mut out, "contract-version", &contract)?;
    meta(&mut out, "compiler-nucleus-sha256", &nucleus_sha256)?;
    meta(&mut out, "artifact-status", "canonical-backend-neutral")?;
    meta(&mut out, "request-count", &requests.len().to_string())?;

    for request in requests {
        let parts = list_values(request)?;
        if parts.len() != 4 {
            return Err(format!(
                "semantic request must have four items, got {}",
                parts.len()
            ));
        }
        let identity = match parts[0] {
            Value::DomainIdentity(identity) => *identity,
            other => return Err(format!("request identity must be DomainIdentity, got {other}")),
        };
        let role = symbol(parts[1], "request role")?;
        let proof_ref = string(parts[2], "request proof-ref")?;
        let provenance = list_values(parts[3])?;
        if provenance.len() != 3 {
            return Err(format!(
                "request provenance must contain three strings, got {}",
                provenance.len()
            ));
        }
        let request_authority_path = string(provenance[0], "request authority path")?;
        let request_authority_sha256 = string(provenance[1], "request authority sha256")?;
        let request_contract = string(provenance[2], "request contract")?;

        if request_authority_path != authority_path
            || request_authority_sha256 != authority_sha256
            || request_contract != contract
        {
            return Err("request provenance disagrees with artifact authority".into());
        }

        let bits = format!(
            "{:0width$b}",
            identity.packed_bits(),
            width = identity.width()
        );
        writeln!(
            out,
            "request\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            identity.width(),
            token(&bits, "identity bits")?,
            token(&role, "role")?,
            token(&proof_ref, "proof ref")?,
            token(&request_authority_path, "request authority path")?,
            token(&request_authority_sha256, "request authority sha256")?,
            token(&request_contract, "request contract")?,
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}
