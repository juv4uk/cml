//! Compiler-owned provenance projection for the x86_64 freestanding backend.
//!
//! The backend already assigns image-local symbol IDs deterministically from a
//! sorted symbol set before it emits assembly. This module exposes that
//! mechanical fact alongside the emitted assembly without giving metadata any
//! authority over admission or language semantics: `compile_program` still
//! runs first and remains the only admission/emission path.

use crate::ir::{Ir, Quoted};
use crate::x86_freestanding::{CompileError, X86FreestandingBackend};
use std::collections::BTreeSet;

/// One compiler-owned x86 target symbol assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X86SymbolMetadata {
    /// The exact canonical spelling used by the x86 backend's symbol map.
    pub name: String,
    /// Image-local target symbol ID.
    pub id: u64,
    /// Exact encoded target word produced by `wsm-os-target` for `id`.
    pub encoded_word: wsm_os_target::Word,
}

/// Assembly plus the deterministic symbol projection that belongs to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X86CompiledProgram {
    pub assembly: String,
    pub symbols: Vec<X86SymbolMetadata>,
}

impl X86CompiledProgram {
    /// Resolve a target Symbol word using only compiler-owned metadata.
    pub fn symbol_name_for_word(&self, word: wsm_os_target::Word) -> Option<&str> {
        self.symbols
            .iter()
            .find(|symbol| symbol.encoded_word == word)
            .map(|symbol| symbol.name.as_str())
    }

    /// Fail-closed structural validation suitable for downstream provenance
    /// consumers before trusting this projection.
    pub fn validate_symbol_metadata(&self) -> bool {
        let canonical_t = match wsm_os_target::encode_symbol(wsm_os_target::SYMBOL_ID_MAX) {
            Some(word) => word,
            None => return false,
        };

        let mut previous_name: Option<&str> = None;
        for (index, symbol) in self.symbols.iter().enumerate() {
            let expected_id = index as u64 + 1;
            if symbol.id != expected_id || symbol.id >= wsm_os_target::SYMBOL_ID_MAX {
                return false;
            }
            if previous_name.is_some_and(|name| name >= symbol.name.as_str()) {
                return false;
            }
            if wsm_os_target::encode_symbol(symbol.id) != Some(symbol.encoded_word) {
                return false;
            }
            if symbol.encoded_word == canonical_t {
                return false;
            }
            previous_name = Some(symbol.name.as_str());
        }
        true
    }
}

impl X86FreestandingBackend {
    /// Compile through the existing backend, then expose the deterministic
    /// target symbol assignment as provenance metadata.
    ///
    /// Metadata never admits an IR node that `compile_program` rejects and is
    /// never fed back into code generation. It is a read-only projection for
    /// consumers such as `wsm-os-lisp` that must render an observed Symbol word
    /// without inventing a second target-side symbol registry.
    pub fn compile_program_with_metadata(
        &self,
        program: &[Ir],
    ) -> Result<X86CompiledProgram, CompileError> {
        let assembly = self.compile_program(program)?;

        let mut names = BTreeSet::new();
        for expression in program {
            collect_backend_symbol_names(expression, &mut names);
        }

        // SYMBOL_ID_MAX is reserved for canonical `t`, so ordinary image-local
        // symbols may use IDs 1..SYMBOL_ID_MAX-1 only. This projection fails
        // closed even if a legacy caller reaches the older assembly-only API.
        if names.len() as u64 >= wsm_os_target::SYMBOL_ID_MAX {
            return Err(CompileError::TooManySymbols);
        }

        let symbols = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let id = index as u64 + 1;
                let encoded_word =
                    wsm_os_target::encode_symbol(id).ok_or(CompileError::TooManySymbols)?;
                Ok(X86SymbolMetadata {
                    name,
                    id,
                    encoded_word,
                })
            })
            .collect::<Result<Vec<_>, CompileError>>()?;

        let output = X86CompiledProgram { assembly, symbols };
        if !output.validate_symbol_metadata() {
            return Err(CompileError::TooManySymbols);
        }
        Ok(output)
    }
}

/// Mirror the x86 backend's existing *mechanical* symbol-set projection after
/// admission has succeeded. Ordinary quoted symbols use the same uppercase
/// canonical spelling as `preflight_quoted`; top-level definition names are
/// included because x86 preflight interns them into the same image-local map.
/// No variable/parameter name is interned merely because it appears in IR.
fn collect_backend_symbol_names(ir: &Ir, out: &mut BTreeSet<String>) {
    match ir {
        Ir::Quote(value) => collect_quoted_symbol_names(value, out),
        Ir::Lambda { body, .. } => collect_backend_symbol_names(body, out),
        Ir::App { func, args } => {
            collect_backend_symbol_names(func, out);
            for arg in args {
                collect_backend_symbol_names(arg, out);
            }
        }
        Ir::Cond { branches } => {
            for (test, body) in branches {
                collect_backend_symbol_names(test, out);
                collect_backend_symbol_names(body, out);
            }
        }
        Ir::Let { bindings, body } => {
            for (_, value) in bindings {
                collect_backend_symbol_names(value, out);
            }
            collect_backend_symbol_names(body, out);
        }
        Ir::Def { name, value } => {
            collect_backend_symbol_names(value, out);
            out.insert(name.clone());
        }
        Ir::Prim { args, .. } | Ir::TailSelfCall { args } => {
            for arg in args {
                collect_backend_symbol_names(arg, out);
            }
        }
        Ir::Int(_)
        | Ir::Float(_)
        | Ir::Rational(_, _)
        | Ir::String(_)
        | Ir::Buffer(_)
        | Ir::Nil
        | Ir::True
        | Ir::Var(_)
        | Ir::Builtin(_) => {}
    }
}

fn collect_quoted_symbol_names(quoted: &Quoted, out: &mut BTreeSet<String>) {
    match quoted {
        // cml#13: exact original spelling, matching the preflight/emission
        // symbol-table key in x86_freestanding.rs -- not the uppercased
        // target-identifier convention other backends use.
        Quoted::Sym { original, .. } => {
            out.insert(original.clone());
        }
        Quoted::List(values) => {
            for value in values {
                collect_quoted_symbol_names(value, out);
            }
        }
        Quoted::DottedList(values, tail) => {
            for value in values {
                collect_quoted_symbol_names(value, out);
            }
            collect_quoted_symbol_names(tail, out);
        }
        Quoted::Int(_)
        | Quoted::Float(_)
        | Quoted::Rational(_, _)
        | Quoted::Str(_)
        | Quoted::Nil => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_quoted_symbol_has_stable_compiler_owned_mapping() {
        let backend = X86FreestandingBackend::new();
        let program = [Ir::Quote(Quoted::Sym {
            uppercased: "RADIO".to_string(),
            original: "radio".to_string(),
        })];

        let first = backend.compile_program_with_metadata(&program).unwrap();
        let second = backend.compile_program_with_metadata(&program).unwrap();
        assert_eq!(first, second);
        assert!(first.validate_symbol_metadata());

        let expected_word = wsm_os_target::encode_symbol(1).unwrap();
        // cml#13: exact my-lisp spelling, not an uppercase reconstruction.
        assert_eq!(
            first.symbols,
            vec![X86SymbolMetadata {
                name: "radio".to_string(),
                id: 1,
                encoded_word: expected_word,
            }]
        );
        assert_eq!(first.symbol_name_for_word(expected_word), Some("radio"));
        assert!(first.assembly.contains(&expected_word.to_string()));
    }

    #[test]
    fn metadata_mutation_is_detectable_and_canonical_t_cannot_be_an_ordinary_symbol() {
        let backend = X86FreestandingBackend::new();
        let program = [Ir::Quote(Quoted::List(vec![
            Quoted::Sym {
                uppercased: "ZETA".to_string(),
                original: "zeta".to_string(),
            },
            Quoted::Sym {
                uppercased: "ALPHA".to_string(),
                original: "alpha".to_string(),
            },
        ]))];
        let compiled = backend.compile_program_with_metadata(&program).unwrap();
        assert_eq!(compiled.symbols[0].name, "alpha");
        assert_eq!(compiled.symbols[1].name, "zeta");
        assert!(compiled.validate_symbol_metadata());

        let mut mutated = compiled.clone();
        mutated.symbols[0].encoded_word =
            wsm_os_target::encode_symbol(wsm_os_target::SYMBOL_ID_MAX).unwrap();
        assert!(!mutated.validate_symbol_metadata());
        assert_ne!(compiled, mutated);
    }
}
