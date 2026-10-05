//! Machine-readable inventory of the compiler passes that already exist.
//!
//! This module does not add language semantics and does not claim that any
//! pass is globally verified.  It names the transformation boundaries so
//! each one can acquire an explicit preservation witness under cml#453.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// Exact structural invariant checked on the transformation itself.
    StructuralInvariant,
    /// Replay against a finite corpus with its bound/provenance recorded.
    BoundedCorpus,
    /// Differential comparison against an independent execution path/oracle.
    Differential,
}

impl EvidenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StructuralInvariant => "structural-invariant",
            Self::BoundedCorpus => "bounded-corpus",
            Self::Differential => "differential",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassDescriptor {
    pub id: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    /// The equality/safety obligation this pass must eventually discharge.
    /// This is an obligation, not a statement that the proof is complete.
    pub obligation: &'static str,
    pub current_evidence: EvidenceKind,
}

/// Ordered list of compiler transformations on the current production path.
///
/// The order mirrors `lower::lower_program` and the optional tail-call
/// lowering already used by the x86 backend.  Keeping this list explicit is a
/// cheap first step toward nanopass-style conformance: later work can attach
/// case digests and proof artifacts to stable pass ids without rewriting the
/// compiler first.
pub const PASS_MANIFEST: &[PassDescriptor] = &[
    PassDescriptor {
        id: "ast.constant-fold.pratyahara",
        input: "parsed-ast",
        output: "folded-ast",
        obligation: "preserve observable meaning while respecting quote as a data barrier",
        current_evidence: EvidenceKind::StructuralInvariant,
    },
    PassDescriptor {
        id: "ast.semantic-admission",
        input: "folded-ast",
        output: "admitted-ast-or-named-error",
        obligation: "admit existing meaning or fail with a named semantic error; never mint meaning",
        current_evidence: EvidenceKind::Differential,
    },
    PassDescriptor {
        id: "ast-to-ir.lower",
        input: "admitted-ast",
        output: "cml-ir",
        obligation: "preserve the upstream observable on every supported mechanism",
        current_evidence: EvidenceKind::BoundedCorpus,
    },
    PassDescriptor {
        id: "ir.tail-self-call",
        input: "cml-ir",
        output: "cml-ir-tail",
        obligation: "rewrite only direct self calls in tail position while preserving arguments and control structure",
        current_evidence: EvidenceKind::StructuralInvariant,
    },
];

pub fn pass_manifest() -> &'static [PassDescriptor] {
    PASS_MANIFEST
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn pass_ids_are_unique_and_obligations_are_nonempty() {
        let mut ids = HashSet::new();
        for pass in PASS_MANIFEST {
            assert!(
                ids.insert(pass.id),
                "duplicate compiler pass id: {}",
                pass.id
            );
            assert!(!pass.input.is_empty());
            assert!(!pass.output.is_empty());
            assert!(!pass.obligation.is_empty());
        }
    }

    #[test]
    fn pass_contract_stays_width_neutral() {
        for pass in PASS_MANIFEST {
            let contract = format!(
                "{} {} {} {}",
                pass.id, pass.input, pass.output, pass.obligation
            )
            .to_ascii_lowercase();
            assert!(
                !contract.contains("sid8") && !contract.contains("sens8"),
                "new pass contract must not canonize legacy 8-bit identity: {}",
                pass.id
            );
        }
    }

    #[test]
    fn current_pass_order_matches_the_existing_lowering_spine() {
        let ids: Vec<_> = PASS_MANIFEST.iter().map(|pass| pass.id).collect();
        assert_eq!(
            ids,
            vec![
                "ast.constant-fold.pratyahara",
                "ast.semantic-admission",
                "ast-to-ir.lower",
                "ir.tail-self-call",
            ]
        );
    }
}
