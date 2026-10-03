//! Compiler-owned provenance projection for the x86_64 freestanding backend.
//!
//! The backend already assigns image-local symbol IDs deterministically from a
//! sorted symbol set before it emits assembly. This module exposes that
//! mechanical fact alongside the emitted assembly without giving metadata any
//! authority over admission or language semantics: `compile_program` still
//! runs first and remains the only admission/emission path.

use crate::canon::{CanonOperation, collect_program_operations, find_operation_by_id};
use crate::ir::{Ir, Quoted};
use crate::x86_freestanding::{CompileError, X86FreestandingBackend};
use std::collections::BTreeSet;

pub const GC_ROOT_MAP_WIRE_VERSION: u32 = 1;

// Research-only runtime payload projection (#443). These values are explicitly
// not a ratified final ABI; they exist so the linked ELF can carry the same
// root locations already proved by the compiler-owned metadata.
pub const GC_ROOT_PAYLOAD_VERSION: u64 = 1;
pub const GC_ROOT_ALLOCATOR_WSM_CONS: u64 = 1;
pub const GC_ROOT_ALLOCATOR_WSM_CLOSURE_NEW: u64 = 2;
pub const GC_ROOT_REG_RSI: u64 = 1 << 0;
pub const GC_ROOT_REG_RDX: u64 = 1 << 1;

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

/// One compiler-proved allocation-site root map projected from the exact
/// assembler certificate emitted by the x86 backend.
///
/// This is mechanism metadata only. It does not admit a language operation,
/// collector, or safepoint beyond what the compiler already proved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X86GcRootMapMetadata {
    pub id: usize,
    pub return_label: String,
    pub allocator: String,
    pub certificate_kind: String,
    pub frame_bytes: usize,
    pub stack_offsets: Vec<usize>,
    pub register_roots: Vec<String>,
}

/// Assembly plus deterministic compiler-owned metadata projections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X86CompiledProgram {
    pub assembly: String,
    pub symbols: Vec<X86SymbolMetadata>,
    pub operations: Vec<&'static CanonOperation>,
    pub gc_root_maps: Vec<X86GcRootMapMetadata>,
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

    /// Fail-closed validation that all operations in metadata match the authoritative
    /// Canon operations table.
    pub fn validate_operation_metadata(&self) -> bool {
        for op in &self.operations {
            if find_operation_by_id(op.semantic_id) != Some(op) {
                return false;
            }
        }
        true
    }

    /// Validate compiler-owned root-map records without making them semantic.
    pub fn validate_gc_root_maps(&self) -> bool {
        let mut ids = BTreeSet::new();
        let mut labels = BTreeSet::new();

        for record in &self.gc_root_maps {
            if !ids.insert(record.id) || !labels.insert(record.return_label.as_str()) {
                return false;
            }
            if record.return_label != format!(".Lgc_return_{}", record.id) {
                return false;
            }
            if record.allocator.is_empty()
                || record.allocator.chars().any(char::is_whitespace)
                || record.certificate_kind.is_empty()
                || record.certificate_kind.chars().any(char::is_whitespace)
            {
                return false;
            }
            if !record
                .stack_offsets
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            {
                return false;
            }
            if record
                .stack_offsets
                .iter()
                .any(|offset| offset % 8 != 0 || *offset >= record.frame_bytes)
            {
                return false;
            }
            if !record
                .register_roots
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            {
                return false;
            }
            if record
                .register_roots
                .iter()
                .any(|register| !matches!(register.as_str(), "%rsi" | "%rdx"))
            {
                return false;
            }

            let call_and_label = format!("    call {}\n{}:", record.allocator, record.return_label);
            if !self.assembly.contains(&call_and_label) {
                return false;
            }
        }

        true
    }

    /// Deterministic research wire projection for a future runtime consumer.
    pub fn gc_root_map_manifest(&self) -> String {
        let mut out = format!("CML_GC_ROOT_MAP_V{}\n", GC_ROOT_MAP_WIRE_VERSION);
        for record in &self.gc_root_maps {
            let stack = if record.stack_offsets.is_empty() {
                "-".to_string()
            } else {
                record
                    .stack_offsets
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let registers = if record.register_roots.is_empty() {
                "-".to_string()
            } else {
                record.register_roots.join(",")
            };
            out.push_str(&format!(
                "site id={} label={} allocator={} kind={} frame={} stack={} regs={}\n",
                record.id,
                record.return_label,
                record.allocator,
                record.certificate_kind,
                record.frame_bytes,
                stack,
                registers
            ));
        }
        out
    }

    fn gc_root_allocator_code(name: &str) -> Result<u64, &'static str> {
        match name {
            "wsm_cons" => Ok(GC_ROOT_ALLOCATOR_WSM_CONS),
            "wsm_closure_new" => Ok(GC_ROOT_ALLOCATOR_WSM_CLOSURE_NEW),
            _ => Err("unsupported GC root allocator for runtime payload"),
        }
    }

    fn gc_root_register_mask(registers: &[String]) -> Result<u64, &'static str> {
        let mut mask = 0u64;
        for register in registers {
            match register.as_str() {
                "%rsi" => mask |= GC_ROOT_REG_RSI,
                "%rdx" => mask |= GC_ROOT_REG_RDX,
                _ => return Err("unsupported GC root register for runtime payload"),
            }
        }
        Ok(mask)
    }

    /// Append a research-only relocation table that binds each already-proved
    /// compiler return label to its final linked PC through the ordinary
    /// assembler/linker relocation mechanism.
    ///
    /// Records are (site_id: u64, final_pc: u64) pairs in
    /// .wsm_gc_root_pc_bind. This is mechanism evidence only: it does not
    /// widen liveness, admit a collector, or ratify the final section ABI.
    pub fn assembly_with_gc_root_pc_bindings(&self) -> Result<String, &'static str> {
        if !self.validate_gc_root_maps() {
            return Err("invalid compiler-owned GC root metadata");
        }

        let mut out = self.assembly.clone();
        if self.gc_root_maps.is_empty() {
            return Ok(out);
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(".section .wsm_gc_root_pc_bind,\"a\",@progbits\n");
        out.push_str(".p2align 3\n");
        for record in &self.gc_root_maps {
            out.push_str(&format!(".quad {}\n", record.id));
            out.push_str(&format!(".quad {}\n", record.return_label));
        }
        Ok(out)
    }

    /// Append both runtime-facing GC research sections:
    ///
    /// - .wsm_gc_root_pc_bind: (site_id, final_pc) relocation pairs;
    /// - .wsm_gc_root_payload: versioned site_id-keyed root payload.
    ///
    /// Payload section encoding (all little-endian u64 after linking):
    ///
    ///   version
    ///   record_count
    ///   repeat record_count times:
    ///     site_id
    ///     frame_bytes
    ///     allocator_code
    ///     register_mask
    ///     stack_root_count
    ///     stack_root_offset[stack_root_count]
    ///
    /// Certificate kind remains host-side diagnostic metadata in this research
    /// slice because traversal does not consume it. Site-id equality is the join
    /// law between the payload and final-PC sections.
    pub fn assembly_with_gc_root_runtime_sections(&self) -> Result<String, &'static str> {
        let mut out = self.assembly_with_gc_root_pc_bindings()?;
        if self.gc_root_maps.is_empty() {
            return Ok(out);
        }

        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(".section .wsm_gc_root_payload,\"a\",@progbits\n");
        out.push_str(".p2align 3\n");
        out.push_str(&format!(".quad {}\n", GC_ROOT_PAYLOAD_VERSION));
        out.push_str(&format!(".quad {}\n", self.gc_root_maps.len()));

        for record in &self.gc_root_maps {
            let allocator_code = Self::gc_root_allocator_code(&record.allocator)?;
            let register_mask = Self::gc_root_register_mask(&record.register_roots)?;

            out.push_str(&format!(".quad {}\n", record.id));
            out.push_str(&format!(".quad {}\n", record.frame_bytes));
            out.push_str(&format!(".quad {}\n", allocator_code));
            out.push_str(&format!(".quad {}\n", register_mask));
            out.push_str(&format!(".quad {}\n", record.stack_offsets.len()));
            for offset in &record.stack_offsets {
                out.push_str(&format!(".quad {}\n", offset));
            }
        }

        Ok(out)
    }
}

impl X86FreestandingBackend {
    /// Compile through the existing backend, then expose the deterministic
    /// target symbol assignment and canonical operation provenance as metadata.
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
        compiled_program_metadata(program, assembly)
    }

    /// Compile through the explicit post-compilation input-entry ABI while
    /// preserving the exact same compiler-owned symbol/operation metadata used
    /// by ordinary x86 witnesses.
    ///
    /// The selected entry name is a mechanism choice supplied by the caller;
    /// metadata remains a read-only projection and does not admit semantics.
    pub fn compile_program_with_metadata_and_input_entry(
        &self,
        program: &[Ir],
        entry_name: &str,
    ) -> Result<X86CompiledProgram, CompileError> {
        let assembly = self.compile_program_with_input_entry(program, entry_name)?;
        compiled_program_metadata(program, assembly)
    }
}

fn compiled_program_metadata(
    program: &[Ir],
    assembly: String,
) -> Result<X86CompiledProgram, CompileError> {
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

    let operations = collect_program_operations(program);
    let gc_root_maps = parse_gc_root_maps_from_assembly(&assembly).ok_or(
        CompileError::UnsupportedVariant("invalid compiler-owned GC root metadata"),
    )?;
    let output = X86CompiledProgram {
        assembly,
        symbols,
        operations,
        gc_root_maps,
    };
    if !output.validate_symbol_metadata()
        || !output.validate_operation_metadata()
        || !output.validate_gc_root_maps()
    {
        return Err(CompileError::UnsupportedVariant(
            "invalid compiler-owned metadata projection",
        ));
    }
    Ok(output)
}

fn metadata_field<'a>(parts: &'a [&'a str], key: &str) -> Option<&'a str> {
    parts
        .iter()
        .filter_map(|part| part.split_once('='))
        .find_map(|(name, value)| (name == key).then_some(value))
}

fn parse_usize_list(raw: &str) -> Option<Vec<usize>> {
    if raw == "-" {
        return Some(Vec::new());
    }
    let mut values = raw
        .split(',')
        .map(str::parse)
        .collect::<Result<Vec<usize>, _>>()
        .ok()?;
    values.sort_unstable();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(values)
}

fn parse_string_list(raw: &str) -> Option<Vec<String>> {
    if raw == "-" {
        return Some(Vec::new());
    }
    let mut values = raw.split(',').map(str::to_string).collect::<Vec<_>>();
    values.sort();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(values)
}

fn parse_gc_root_maps_from_assembly(assembly: &str) -> Option<Vec<X86GcRootMapMetadata>> {
    let mut records = Vec::<X86GcRootMapMetadata>::new();

    for raw in assembly.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("# GC_SAFEPOINT ") {
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            let id = metadata_field(&parts, "id")?.parse().ok()?;
            let certificate_kind = metadata_field(&parts, "kind")?.to_string();
            let allocator = metadata_field(&parts, "allocator")?.to_string();
            let frame_bytes = metadata_field(&parts, "frame")?.parse().ok()?;
            let return_label = metadata_field(&parts, "return_label")?.to_string();

            if records
                .iter()
                .any(|record| record.id == id || record.return_label == return_label)
            {
                return None;
            }

            records.push(X86GcRootMapMetadata {
                id,
                return_label,
                allocator,
                certificate_kind,
                frame_bytes,
                stack_offsets: Vec::new(),
                register_roots: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("# GC_STACK_ROOT ") {
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            let id = metadata_field(&parts, "id")?.parse().ok()?;
            let offset = metadata_field(&parts, "offset")?.parse().ok()?;
            let current = records.last_mut()?;
            if current.id != id || current.stack_offsets.contains(&offset) {
                return None;
            }
            current.stack_offsets.push(offset);
        } else if let Some(rest) = line.strip_prefix("# GC_REGISTER_ROOT ") {
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            let id = metadata_field(&parts, "id")?.parse().ok()?;
            let register = metadata_field(&parts, "reg")?.to_string();
            let current = records.last_mut()?;
            if current.id != id || current.register_roots.contains(&register) {
                return None;
            }
            current.register_roots.push(register);
        }
    }

    for record in &mut records {
        record.stack_offsets.sort_unstable();
        record.register_roots.sort();
    }
    records.sort_by_key(|record| record.id);
    Some(records)
}

/// Parse the deterministic research manifest emitted by the compiled-program
/// root-map projection. This does not authorize collection or invent a safepoint.
pub fn parse_gc_root_map_manifest(
    manifest: &str,
) -> Result<Vec<X86GcRootMapMetadata>, &'static str> {
    let mut lines = manifest.lines();
    let expected_header = format!("CML_GC_ROOT_MAP_V{}", GC_ROOT_MAP_WIRE_VERSION);
    if lines.next() != Some(expected_header.as_str()) {
        return Err("unsupported GC root-map manifest version");
    }

    let mut records = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let rest = line
            .strip_prefix("site ")
            .ok_or("invalid GC root-map record prefix")?;
        let parts = rest.split_whitespace().collect::<Vec<_>>();
        let id = metadata_field(&parts, "id")
            .ok_or("missing site id")?
            .parse()
            .map_err(|_| "invalid site id")?;
        let return_label = metadata_field(&parts, "label")
            .ok_or("missing return label")?
            .to_string();
        let allocator = metadata_field(&parts, "allocator")
            .ok_or("missing allocator")?
            .to_string();
        let certificate_kind = metadata_field(&parts, "kind")
            .ok_or("missing certificate kind")?
            .to_string();
        let frame_bytes = metadata_field(&parts, "frame")
            .ok_or("missing frame size")?
            .parse()
            .map_err(|_| "invalid frame size")?;
        let stack_offsets =
            parse_usize_list(metadata_field(&parts, "stack").ok_or("missing stack roots")?)
                .ok_or("invalid stack roots")?;
        let register_roots =
            parse_string_list(metadata_field(&parts, "regs").ok_or("missing register roots")?)
                .ok_or("invalid register roots")?;

        let record = X86GcRootMapMetadata {
            id,
            return_label,
            allocator,
            certificate_kind,
            frame_bytes,
            stack_offsets,
            register_roots,
        };
        if record.allocator.is_empty()
            || record.certificate_kind.is_empty()
            || record.allocator.chars().any(char::is_whitespace)
            || record.certificate_kind.chars().any(char::is_whitespace)
        {
            return Err("empty or invalid GC root-map token");
        }
        if records.iter().any(|existing: &X86GcRootMapMetadata| {
            existing.id == record.id || existing.return_label == record.return_label
        }) {
            return Err("duplicate GC root-map site");
        }
        if record.return_label != format!(".Lgc_return_{}", record.id) {
            return Err("return label does not match site id");
        }
        if record
            .stack_offsets
            .iter()
            .any(|offset| offset % 8 != 0 || *offset >= record.frame_bytes)
        {
            return Err("invalid stack-root offset");
        }
        if record
            .register_roots
            .iter()
            .any(|register| !matches!(register.as_str(), "%rsi" | "%rdx"))
        {
            return Err("unsupported rewriteable register");
        }
        records.push(record);
    }

    records.sort_by_key(|record| record.id);
    Ok(records)
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
        Ir::CondMatch { branches } => {
            for (query, expected, body) in branches {
                collect_backend_symbol_names(query, out);
                collect_quoted_symbol_names(expected, out);
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
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } | Ir::TailSelfCall { args } => {
            for arg in args {
                collect_backend_symbol_names(arg, out);
            }
        }
        Ir::Sid(_)
        | Ir::Int(_)
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

    #[test]
    fn operations_metadata_records_canonical_identity_and_provenance() {
        let backend = X86FreestandingBackend::new();
        let program = [
            Ir::Quote(Quoted::Int(42)),
            Ir::App {
                func: Box::new(Ir::Sid(sens::sid!(00001100))),
                args: vec![Ir::Int(1), Ir::Int(2)],
            },
            Ir::App {
                func: Box::new(Ir::Sid(sens::sid!(00000100))),
                args: vec![Ir::Int(1), Ir::Nil],
            },
        ];
        let compiled = backend.compile_program_with_metadata(&program).unwrap();
        assert!(compiled.validate_operation_metadata());
        let op_ids: Vec<sens::Sid8> = compiled
            .operations
            .iter()
            .map(|op| op.semantic_id)
            .collect();
        assert_eq!(
            op_ids,
            vec![
                sens::sid!(00000001),
                sens::sid!(00001100),
                sens::sid!(00000100)
            ],
            "operation metadata must preserve first encounter order, not impose an ordering on opaque Sid8 identities"
        );
        assert_eq!(compiled.operations[0].canonical_name, "quote");
        assert_eq!(compiled.operations[1].canonical_name, "plus");
        assert_eq!(compiled.operations[2].canonical_name, "cons");
    }
}
