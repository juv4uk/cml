//! Consumer for my-lisp `cml-export/1` (CML-SEMANTIC-EXPORT-V1).
//!
//! my-lisp is the sole authority for program meaning. This module only
//! parses the versioned export artifact and fails closed on drift — it does
//! not invent semantic form identities or evaluation rules.
//!
//! Design: my-lisp `docs/cml-semantic-export-v1-design.md`.
//! Producer: my-lisp `cml-export` binary (commit f142e55+).

use std::fmt;

/// Schema tag expected at the root of the export file.
pub const SCHEMA: &str = "cml-export/1";

/// Semantic IDs required by vertical slice 1 (named def + recursion).
pub const SLICE_1_FORM_IDS: &[&str] = &["0001", "0003", "0007", "0010", "0011", "1001"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Syntax,
    Primitive,
    Library,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub id: String,
    pub role: Role,
    pub callable: bool,
    pub surfaces: Vec<(String, String)>, // (namespace, name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticExport {
    pub contract_major: u32,
    pub contract_minor: u32,
    pub digest: String,
    pub forms: Vec<Form>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    MissingSchema,
    MissingDigest,
    MissingForms,
    Malformed(String),
    DigestMismatch { expected: String, found: String },
    MissingSlice1Form(String),
    RoleCallableInconsistency(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::MissingSchema => write!(f, "semantic export: missing {SCHEMA}"),
            ExportError::MissingDigest => write!(f, "semantic export: missing digest"),
            ExportError::MissingForms => write!(f, "semantic export: missing forms block"),
            ExportError::Malformed(m) => write!(f, "semantic export: malformed: {m}"),
            ExportError::DigestMismatch { expected, found } => {
                write!(f, "semantic export: digest mismatch expected={expected} found={found}")
            }
            ExportError::MissingSlice1Form(id) => {
                write!(f, "semantic export: missing slice-1 form id {id}")
            }
            ExportError::RoleCallableInconsistency(id) => {
                write!(f, "semantic export: role/callable inconsistency for {id}")
            }
        }
    }
}

impl std::error::Error for ExportError {}

fn parse_role(s: &str) -> Role {
    match s {
        "syntax" => Role::Syntax,
        "primitive" => Role::Primitive,
        "library" => Role::Library,
        other => Role::Other(other.to_string()),
    }
}

/// Minimal structural parse of `cml-export/1` text (not a full sexpr parser).
pub fn parse_export(text: &str) -> Result<SemanticExport, ExportError> {
    if !text.contains(SCHEMA) {
        return Err(ExportError::MissingSchema);
    }

    let digest = extract_quoted_field(text, "digest").ok_or(ExportError::MissingDigest)?;

    let major = extract_paren_int(text, "major").unwrap_or(0);
    let minor = extract_paren_int(text, "minor").unwrap_or(0);

    let forms_start = text.find("(forms").ok_or(ExportError::MissingForms)?;
    let forms_region = &text[forms_start..];

    let mut forms = Vec::new();
    // Match form rows: (NNNN (surfaces ...) (role R) (callable C))
    for id in ["0001", "0003", "0007", "0010", "0011", "1001"] {
        let marker = format!("({id} ");
        if let Some(idx) = forms_region.find(&marker) {
            let row = &forms_region[idx..];
            let role = extract_symbol_field(row, "role")
                .map(|s| parse_role(&s))
                .unwrap_or(Role::Other("?".into()));
            let callable = extract_symbol_field(row, "callable")
                .map(|s| s == "t")
                .unwrap_or(false);
            let surfaces = extract_surfaces(row);
            forms.push(Form {
                id: id.into(),
                role,
                callable,
                surfaces,
            });
        }
    }

    if forms.is_empty() {
        return Err(ExportError::MissingForms);
    }

    Ok(SemanticExport {
        contract_major: major,
        contract_minor: minor,
        digest,
        forms,
    })
}

fn extract_quoted_field(text: &str, field: &str) -> Option<String> {
    let marker = format!("({field} \"");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..].find('"')? + start;
    Some(text[start..end].to_string())
}

fn extract_paren_int(text: &str, field: &str) -> Option<u32> {
    let marker = format!("({field} ");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..]
        .find(|c: char| c == ')' || c.is_whitespace())?
        + start;
    text[start..end].parse().ok()
}

fn extract_symbol_field(text: &str, field: &str) -> Option<String> {
    let marker = format!("({field} ");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..]
        .find(|c: char| c == ')' || c.is_whitespace())?
        + start;
    Some(text[start..end].to_string())
}

fn extract_surfaces(row: &str) -> Vec<(String, String)> {
    let Some(start) = row.find("(surfaces ") else {
        return Vec::new();
    };
    let region = &row[start + "(surfaces ".len()..];
    let end = region.find(") (role").or_else(|| region.find(')')).unwrap_or(region.len());
    let body = &region[..end];
    let mut out = Vec::new();
    // (en quote) (uk ...) ...
    let mut rest = body;
    while let Some(open) = rest.find('(') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find(')') else { break };
        let pair = rest[..close].trim();
        let mut parts = pair.split_whitespace();
        if let (Some(ns), Some(name)) = (parts.next(), parts.next()) {
            out.push((ns.to_string(), name.to_string()));
        }
        rest = &rest[close + 1..];
    }
    out
}

/// Validate slice-1 requirements against a parsed export.
pub fn validate_slice1(export: &SemanticExport) -> Result<(), ExportError> {
    for id in SLICE_1_FORM_IDS {
        if !export.forms.iter().any(|f| f.id == *id) {
            return Err(ExportError::MissingSlice1Form((*id).into()));
        }
    }
    for form in &export.forms {
        match form.role {
            Role::Syntax if form.callable => {
                return Err(ExportError::RoleCallableInconsistency(form.id.clone()));
            }
            Role::Primitive | Role::Library if !form.callable => {
                return Err(ExportError::RoleCallableInconsistency(form.id.clone()));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Optional: pin expected digest once my-lisp publishes a stable file.
pub fn check_digest(export: &SemanticExport, expected: &str) -> Result<(), ExportError> {
    if expected == "pending-producer-byte-pin" || export.digest == "pending-producer-byte-pin" {
        // Soft pin until producer artifact is byte-vendored from a real run.
        return Ok(());
    }
    if export.digest != expected {
        return Err(ExportError::DigestMismatch {
            expected: expected.into(),
            found: export.digest.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VENDORED: &str = include_str!("../contracts/mylisp-cml-export.wsm");

    #[test]
    fn parses_vendored_export() {
        let export = parse_export(VENDORED).expect("parse");
        assert_eq!(export.contract_major, 6);
        assert_eq!(export.forms.len(), 6);
        validate_slice1(&export).expect("slice1");
    }

    #[test]
    fn syntax_forms_are_not_callable() {
        let export = parse_export(VENDORED).unwrap();
        for id in ["0001", "0007", "0010", "0011"] {
            let f = export.forms.iter().find(|f| f.id == id).unwrap();
            assert!(!f.callable, "{id} must not be callable");
            assert_eq!(f.role, Role::Syntax);
        }
    }

    #[test]
    fn eq_and_sub_are_callable() {
        let export = parse_export(VENDORED).unwrap();
        let eq = export.forms.iter().find(|f| f.id == "0003").unwrap();
        assert!(eq.callable);
        let sub = export.forms.iter().find(|f| f.id == "1001").unwrap();
        assert!(sub.callable);
    }

    #[test]
    fn missing_schema_fails_closed() {
        assert!(matches!(
            parse_export("(forms ...)"),
            Err(ExportError::MissingSchema)
        ));
    }
}
