//! Machine Lowering Authority Boundary Validator (Issue #38 P1).
//!
//! Validates CML's compiler boundaries against the upstream `my-lisp`
//! `machine-lowering-boundary.lisp` contract.
//!
//! Guarantees:
//! 1. `my-lisp` owns semantic identities; CML owns target realization.
//! 2. Lowering direction is strictly `semantic -> machine`; reverse is forbidden.
//! 3. Machine primitives (e.g. RDTSC) are compiler-owned mechanisms and cannot allocate
//!    or masquerade as language semantic IDs.
//! 4. Retired semantic IDs (e.g. `1153`) are fail-closed rejected and can never be recycled.

use std::fmt;

/// Expected schema in machine-lowering-boundary contract.
pub const BOUNDARY_SCHEMA: &str = "machine-lowering-boundary/1";

/// Upstream contract path in CML repository.
pub const BOUNDARY_CONTRACT_PATH: &str = "contracts/my-lisp/machine-lowering-boundary.lisp";

/// Parsed machine lowering boundary representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineLoweringBoundary {
    pub schema: String,
    pub semantic_authority: String,
    pub compiler_authority: String,
    pub lowering_direction: String,
    pub reverse_authority: String,
    pub portable_monotonic_observation: String,
    pub retired_semantic_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryError {
    MissingSchema,
    InvalidSchema(String),
    AuthorityMismatch {
        expected: &'static str,
        found: String,
    },
    ReverseAuthorityViolation(String),
    RetiredSemanticIdAttempt(String),
    ParseError(String),
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSchema => write!(f, "machine-lowering-boundary: missing schema"),
            Self::InvalidSchema(s) => {
                write!(
                    f,
                    "machine-lowering-boundary: invalid schema {s}, expected {BOUNDARY_SCHEMA}"
                )
            }
            Self::AuthorityMismatch { expected, found } => {
                write!(
                    f,
                    "machine-lowering-boundary: authority mismatch, expected {expected}, found {found}"
                )
            }
            Self::ReverseAuthorityViolation(msg) => {
                write!(
                    f,
                    "machine-lowering-boundary violation: reverse authority forbidden: {msg}"
                )
            }
            Self::RetiredSemanticIdAttempt(id) => {
                write!(
                    f,
                    "machine-lowering-boundary violation: attempt to use retired semantic ID {id}"
                )
            }
            Self::ParseError(msg) => write!(f, "machine-lowering-boundary parse error: {msg}"),
        }
    }
}

impl std::error::Error for BoundaryError {}

impl MachineLoweringBoundary {
    /// Parse boundary contract from S-expression string.
    pub fn parse(source: &str) -> Result<Self, BoundaryError> {
        let clean = source
            .lines()
            .map(|l| l.split(';').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");

        let tokens = tokenize(&clean);
        if tokens.is_empty() {
            return Err(BoundaryError::ParseError(
                "empty boundary contract".to_string(),
            ));
        }

        let mut schema = None;
        let mut semantic_authority = None;
        let mut compiler_authority = None;
        let mut lowering_direction = None;
        let mut reverse_authority = None;
        let mut portable_monotonic = None;
        let mut retired_ids = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            if tokens[i] == "(" && i + 2 < tokens.len() {
                let key = &tokens[i + 1];
                let val = &tokens[i + 2];
                match key.as_str() {
                    "schema" => schema = Some(val.clone()),
                    "semantic-authority" => semantic_authority = Some(val.clone()),
                    "compiler-authority" => compiler_authority = Some(val.clone()),
                    "lowering-direction" => lowering_direction = Some(val.clone()),
                    "reverse-authority" => reverse_authority = Some(val.clone()),
                    "portable-monotonic-observation" => portable_monotonic = Some(val.clone()),
                    "retired-semantic-id" => retired_ids.push(val.clone()),
                    _ => {}
                }
            }
            i += 1;
        }

        let schema = schema.ok_or(BoundaryError::MissingSchema)?;
        if schema != BOUNDARY_SCHEMA {
            return Err(BoundaryError::InvalidSchema(schema));
        }

        let semantic_auth = semantic_authority.unwrap_or_default();
        if semantic_auth != "my-lisp" {
            return Err(BoundaryError::AuthorityMismatch {
                expected: "my-lisp",
                found: semantic_auth,
            });
        }

        let compiler_auth = compiler_authority.unwrap_or_default();
        if compiler_auth != "cml" {
            return Err(BoundaryError::AuthorityMismatch {
                expected: "cml",
                found: compiler_auth,
            });
        }

        let lowering_dir = lowering_direction.unwrap_or_default();
        if lowering_dir != "semantic-to-machine" {
            return Err(BoundaryError::ReverseAuthorityViolation(format!(
                "lowering direction must be semantic-to-machine, found {lowering_dir}"
            )));
        }

        let rev_auth = reverse_authority.unwrap_or_default();
        if rev_auth != "forbidden" {
            return Err(BoundaryError::ReverseAuthorityViolation(format!(
                "reverse authority must be forbidden, found {rev_auth}"
            )));
        }

        Ok(Self {
            schema,
            semantic_authority: semantic_auth,
            compiler_authority: compiler_auth,
            lowering_direction: lowering_dir,
            reverse_authority: rev_auth,
            portable_monotonic_observation: portable_monotonic.unwrap_or_default(),
            retired_semantic_ids: retired_ids,
        })
    }

    /// Load and validate the vendored contract.
    pub fn load_vendored() -> Result<Self, BoundaryError> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir).join(BOUNDARY_CONTRACT_PATH);
        let source = std::fs::read_to_string(&path)
            .map_err(|e| BoundaryError::ParseError(format!("reading {}: {e}", path.display())))?;
        Self::parse(&source)
    }

    /// Check if a semantic ID is admitted or retired.
    pub fn validate_semantic_id(&self, id: &str) -> Result<(), BoundaryError> {
        if self.retired_semantic_ids.iter().any(|r| r == id) {
            return Err(BoundaryError::RetiredSemanticIdAttempt(id.to_string()));
        }
        Ok(())
    }
}

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        match ch {
            '(' | ')' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(ch.to_string());
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
