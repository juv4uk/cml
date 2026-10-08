//! Deterministic GNU x86_64 assembly emitter for the `wsm-os` target ABI.
//!
//! This is deliberately a narrow first slice. It consumes admitted shared
//! [`crate::ir::Ir`] and the pinned machine-readable `wsm-os-target` crate.
//! Unsupported IR is rejected during preflight, before any assembly text is
//! produced. There is no libc, host syscall, filesystem, or C-backend fallback.

use crate::compiler_mechanism::RichCompilerMechanismRef;
use crate::ir::{Ir, MachineOp, Params, PrimOp, Quoted};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Canonical `t`, as an ordinary interned Symbol -- not the standalone
/// `Tag::True` primitive canonical WSM never had (`t` is plain
/// `Symbol("t")` in the Rust oracle, not a distinct primitive). Same value
/// `wsm-os-runtime::CANONICAL_T` already computes (2026-09-02 fix) for
/// eq/atom results; this crate emits it directly at compile time for the
/// literal `Ir::True` case, closing the gap that fix left open (see
/// wsm-os-runtime's own doc comment: "`Tag::True` itself stays declared...
/// nothing in this crate produces it anymore" -- x86_freestanding did,
/// until now). `wsm_os_target::TRUE` (the raw immediate) is deliberately
/// no longer emitted here.
const CANONICAL_T: wsm_os_target::Word =
    match wsm_os_target::encode_symbol(wsm_os_target::SYMBOL_ID_MAX) {
        Some(word) => word,
        None => panic!("SYMBOL_ID_MAX must encode as a valid symbol word"),
    };

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    EmptyProgram,
    UnsupportedVariant(&'static str),
    UnimplementedSid8(sens::Sid8),
    UnsupportedCompilerMechanism(RichCompilerMechanismRef),
    InvalidArity {
        operation: &'static str,
        expected: usize,
        actual: usize,
    },
    DefArityMismatch {
        name: String,
        expected: usize,
        actual: usize,
    },
    FixnumOutOfRange(i64),
    TooManySymbols,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefArity {
    Fixed(usize),
    Variadic { fixed: usize },
    AllRest,
    Data,
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyProgram => write!(formatter, "empty program has no target value"),
            Self::UnsupportedVariant(node) => {
                write!(
                    formatter,
                    "unsupported IR in x86_64-freestanding backend: {node}"
                )
            }
            Self::UnimplementedSid8(sid) => {
                write!(
                    formatter,
                    "unimplemented SID8 call in x86_64-freestanding backend: {sid}"
                )
            }
            Self::UnsupportedCompilerMechanism(mechanism) => write!(
                formatter,
                "verified current SENS mechanism has no admitted x86_64-freestanding projection: {}",
                mechanism.as_str()
            ),
            Self::InvalidArity {
                operation,
                expected,
                actual,
            } => write!(
                formatter,
                "{operation} expects {expected} argument(s), got {actual}"
            ),
            Self::DefArityMismatch {
                name,
                expected,
                actual,
            } => write!(
                formatter,
                "named function {name} expects {expected} argument(s), got {actual}"
            ),
            Self::FixnumOutOfRange(value) => {
                write!(formatter, "fixnum outside wsm-os target range: {value}")
            }
            Self::TooManySymbols => write!(formatter, "symbol table exceeds wsm-os target range"),
        }
    }
}

impl std::error::Error for CompileError {}

/// cml#12: ordinary image-local symbols are assigned `index + 1`; canonical
/// `t` is reserved as `Symbol(SYMBOL_ID_MAX)` alone. A table of exactly
/// `SYMBOL_ID_MAX` ordinary symbols would give the last one that same
/// identity, so the last admissible ordinary count is `SYMBOL_ID_MAX - 1` --
/// reject at `>=`, not `>`. Shared by both the flat and tail-call symbol-
/// admission paths so the invariant cannot drift between them.
fn check_symbol_capacity(count: u64) -> Result<(), CompileError> {
    if count >= wsm_os_target::SYMBOL_ID_MAX {
        return Err(CompileError::TooManySymbols);
    }
    Ok(())
}

fn current_compiler_runtime_contract(
    mechanism: RichCompilerMechanismRef,
) -> Option<(usize, &'static str)> {
    match mechanism {
        RichCompilerMechanismRef::SelectorTail => Some((1, "wsm_cdr")),
        RichCompilerMechanismRef::SelectorHead => Some((1, "wsm_car")),
        RichCompilerMechanismRef::PairConstruct => Some((2, "wsm_cons")),
        // Current exact predicates and COND use dedicated emitters below.
        // They are not ordinary runtime calls and therefore stay absent from
        // this structural runtime-contract table.
        RichCompilerMechanismRef::AtomPredicateD1
        | RichCompilerMechanismRef::AtomEqualityD1
        | RichCompilerMechanismRef::ConditionalD1
        | RichCompilerMechanismRef::Quote
        | RichCompilerMechanismRef::Lambda
        | RichCompilerMechanismRef::Define => None,
    }
}

fn checked_current_compiler_runtime(
    mechanism: RichCompilerMechanismRef,
    actual: usize,
) -> Result<&'static str, CompileError> {
    let Some((expected, runtime)) = current_compiler_runtime_contract(mechanism) else {
        return Err(CompileError::UnsupportedCompilerMechanism(mechanism));
    };
    if actual != expected {
        return Err(CompileError::InvalidArity {
            operation: mechanism.as_str(),
            expected,
            actual,
        });
    }
    Ok(runtime)
}

fn checked_current_predicate_mechanism(
    mechanism: RichCompilerMechanismRef,
    actual: usize,
) -> Result<(), CompileError> {
    let expected = match mechanism {
        RichCompilerMechanismRef::AtomPredicateD1 => 1,
        RichCompilerMechanismRef::AtomEqualityD1 => 2,
        _ => return Err(CompileError::UnsupportedCompilerMechanism(mechanism)),
    };
    if actual != expected {
        return Err(CompileError::InvalidArity {
            operation: mechanism.as_str(),
            expected,
            actual,
        });
    }
    Ok(())
}

fn checked_current_conditional_mechanism(
    mechanism: RichCompilerMechanismRef,
    actual: usize,
) -> Result<(), CompileError> {
    if mechanism != RichCompilerMechanismRef::ConditionalD1 {
        return Err(CompileError::UnsupportedCompilerMechanism(mechanism));
    }
    if actual % 2 != 0 {
        return Err(CompileError::UnsupportedVariant(
            "current exact-D1 COND requires consecutive test/body pairs",
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct X86FreestandingBackend;

impl X86FreestandingBackend {
    pub fn new() -> Self {
        Self
    }

    /// Compile a complete program and expose one explicitly selected top-level
    /// unary definition as a post-compilation native value-input entry.
    ///
    /// This is an ABI/mechanism projection only. The caller names the already
    /// admitted Lisp definition; CML does not choose language meaning here.
    /// The wrapper follows the existing named-function convention:
    ///   %rdi = runtime context, %rsi = one target value input.
    pub fn compile_program_with_input_entry(
        &self,
        program: &[Ir],
        entry_name: &str,
    ) -> Result<String, CompileError> {
        let requested = entry_name.to_uppercase();
        let mut function_label = 0_usize;
        let mut selected_label = None;

        for expression in program {
            let Ir::Def { name, value } = expression else {
                continue;
            };
            let Ir::Lambda { params, .. } = value.as_ref() else {
                continue;
            };

            if name == &requested {
                match params {
                    Params::Fixed(names) if names.len() == 1 => {
                        selected_label = Some(function_label);
                    }
                    _ => {
                        return Err(CompileError::UnsupportedVariant(
                            "native input entry must be a fixed unary top-level definition",
                        ));
                    }
                }
            }
            function_label += 1;
        }

        let selected_label = selected_label.ok_or(CompileError::UnsupportedVariant(
            "requested native input entry is not a top-level unary definition",
        ))?;

        let mut assembly = self.compile_program(program)?;
        assembly.push_str("\n.text\n");
        assembly.push_str(".globl wsm_entry_with_input\n");
        assembly.push_str(".type wsm_entry_with_input, @function\n");
        assembly.push_str("wsm_entry_with_input:\n");
        assembly.push_str("    pushq %r12\n");
        assembly.push_str("    subq $16, %rsp\n");
        assembly.push_str("    movq %rdi, %r12\n");
        assembly.push_str("    movq %rsi, 0(%rsp)\n");
        // Reuse the generated program entry for its one-time-per-invocation
        // startup mechanics: named first-class closure slots and data defs
        // must be initialized before a selected definition can observe them.
        // Preserve the post-compile input across that ordinary program entry.
        assembly.push_str("    call wsm_entry\n");
        assembly.push_str("    movq 0(%rsp), %rsi\n");
        assembly.push_str("    movq %r12, %rdi\n");
        assembly.push_str(&format!("    call .Lfn_{selected_label}\n"));
        assembly.push_str("    addq $16, %rsp\n");
        assembly.push_str("    popq %r12\n");
        assembly.push_str("    ret\n");
        assembly.push_str(".size wsm_entry_with_input, .-wsm_entry_with_input\n");
        assembly.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        Ok(assembly)
    }

    /// Compile a complete program. Validation and symbol assignment finish
    /// before the output buffer is created, so every error is fail-closed.
    pub fn compile_program(&self, program: &[Ir]) -> Result<String, CompileError> {
        if program.is_empty() {
            return Err(CompileError::EmptyProgram);
        }

        // Detect the tail-call program shape:
        //   [Def { name, Lambda { params: Fixed([p]), body } },
        //    App { func: Var(name), args: [initial_arg] }]
        //
        // This is the only first-order self-tail-call pattern admitted by the
        // x86 freestanding backend. All other shapes fall through to the flat
        // preflight path, which rejects Def/Lambda/App as unsupported.
        if let [
            Ir::Def {
                name: def_name,
                value,
            },
            Ir::App {
                func,
                args: call_args,
            },
        ] = program
        {
            if let (
                Ir::Lambda {
                    params: Params::Fixed(param_names),
                    body,
                },
                Ir::Var(call_name),
            ) = (value.as_ref(), func.as_ref())
            {
                if call_name == def_name
                    && call_args.len() == param_names.len()
                    && contains_tail_self_call(body)
                {
                    return self.compile_tail_call_program(def_name, param_names, body, call_args);
                }
            }
        }

        // Flat (non-tail-call) program path.
        let mut symbol_names = BTreeSet::new();
        // Declaration pass: every top-level fixed-arity definition is known
        // before body validation. This admits a later definition as a named
        // call target while still rejecting a bare function value unless the
        // Stage2 first-class unary-closure gate below explicitly admits it.
        let mut def_arities = BTreeMap::new();
        for expression in program {
            if let Ir::Def { name, value } = expression {
                match value.as_ref() {
                    Ir::Lambda {
                        params: Params::Fixed(params),
                        ..
                    } => {
                        def_arities.insert(name.clone(), DefArity::Fixed(params.len()));
                    }
                    Ir::Lambda {
                        params: Params::Variadic { fixed, .. },
                        ..
                    } => {
                        def_arities.insert(name.clone(), DefArity::Variadic { fixed: fixed.len() });
                    }
                    Ir::Lambda {
                        params: Params::AllRest(_),
                        ..
                    } => {
                        def_arities.insert(name.clone(), DefArity::AllRest);
                    }
                    _ => {
                        // cml#8: a data-only def is a known name (Var reads of it
                        // are fine) but not callable (Data, distinct from an
                        // unknown name which is simply absent from the map).
                        def_arities.insert(name.clone(), DefArity::Data);
                    }
                }
            }
        }
        let first_class_named_functions =
            collect_first_class_named_functions(program, &def_arities)?;
        let mut slots = 0_usize;
        for expression in program {
            preflight(expression, &mut symbol_names, &mut def_arities, &mut slots)?;
        }
        check_symbol_capacity(symbol_names.len() as u64)?;
        let symbols: BTreeMap<String, u64> = symbol_names
            .into_iter()
            .enumerate()
            .map(|(index, name)| (name, index as u64 + 1))
            .collect();

        // Entry RSP is 8 mod 16. Pushing the one callee-saved register makes
        // every subsequent runtime call correctly 16-byte aligned. Keep the
        // preallocated spill frame a multiple of 16 so that remains true.
        let frame_bytes = slots.next_multiple_of(2) * 8;
        let mut emitter = Emitter {
            output: String::new(),
            symbols,
            env: BTreeMap::new(),
            next_slot: 0,
            next_label: 0,
            frame_bytes,
            next_gc_safepoint: 0,
            gc_emit_depth: 0,
            gc_structured_live_slots: Vec::new(),
            gc_structured_context_complete: false,
            closure_labels: BTreeMap::new(),
            named_closure_definitions: BTreeMap::new(),
            functions: BTreeMap::new(),
            function_arities: def_arities.clone(),
            data_defs: BTreeMap::new(),
        };
        // A top-level definition is executable code, not an expression to
        // fall through while `wsm_entry` is running. Reserve every entry
        // label before emitting the entry body, so calls are source-order
        // independent and bodies can live after `wsm_entry`'s `ret`.
        for expression in program {
            if let Ir::Def { name, value } = expression {
                if matches!(value.as_ref(), Ir::Lambda { .. }) {
                    let label = emitter.allocate_label();
                    emitter.functions.insert(name.clone(), label);
                }
            }
        }
        // cml#8: a top-level def whose value is not a lambda at all gets a
        // dedicated word slot instead of a `.Lfn_N` label -- it has no body
        // to call, only a value to evaluate once and re-read. Initializers
        // run in program order at startup, right after closure descriptors
        // below, so a data def may reference an earlier data def or an
        // already-registered function but not a later data def.
        let data_def_exprs: Vec<(String, Ir)> = program
            .iter()
            .filter_map(|expression| match expression {
                Ir::Def { name, value } if !matches!(value.as_ref(), Ir::Lambda { .. }) => {
                    Some((name.clone(), value.as_ref().clone()))
                }
                _ => None,
            })
            .collect();
        for (name, _) in &data_def_exprs {
            let definition_id = emitter.allocate_label();
            emitter.data_defs.insert(name.clone(), definition_id);
        }
        // Only top-level fixed-arity defs (0..=5) that are actually read as
        // values receive a closure identity. Direct calls stay direct
        // `.Lfn_N` calls.
        // One descriptor is allocated at wsm_entry startup and re-read from a
        // generated word slot, so two reads of the same binding preserve `eq`
        // identity. This is the exact machine property meta-eval's provenance
        // tokens require; allocating a fresh descriptor per Var read would be
        // semantically wrong even if definition/environment payloads matched.
        for name in first_class_named_functions {
            let arity = match def_arities.get(&name) {
                Some(DefArity::Fixed(arity)) if *arity <= 5 => *arity,
                _ => unreachable!(
                    "first-class named function collection admitted a non-fixed/bounded def"
                ),
            };
            let definition_id = emitter.allocate_label() + 1;
            emitter.closure_labels.insert(definition_id, arity);
            emitter
                .named_closure_definitions
                .insert(name, definition_id);
        }

        emitter.line(".text");
        emitter.line(".globl wsm_entry");
        emitter.line(".type wsm_entry, @function");
        emitter.line("wsm_entry:");
        emitter.line("    pushq %r12");
        if frame_bytes != 0 {
            emitter.line(&format!("    subq ${frame_bytes}, %rsp"));
        }
        emitter.line("    movq %rdi, %r12");
        let named_closure_ids: Vec<usize> = emitter
            .named_closure_definitions
            .values()
            .copied()
            .collect();
        for definition_id in &named_closure_ids {
            emitter.line("    movq %r12, %rdi");
            emitter.line(&format!("    movl ${definition_id}, %esi"));
            emitter.line(&format!("    movabsq ${}, %rdx", wsm_os_target::NIL));
            emitter.line("    call wsm_closure_new");
            emitter.line(&format!(
                "    movq %rax, .Lnamed_closure_word_{definition_id}(%rip)"
            ));
        }
        for (name, value) in &data_def_exprs {
            emitter.emit_ir(value)?;
            let definition_id = emitter.data_defs[name];
            emitter.line(&format!("    movq %rax, .Ldata_word_{definition_id}(%rip)"));
        }
        let mut has_entry_expression = false;
        for expression in program {
            if !matches!(expression, Ir::Def { .. }) {
                emitter.emit_ir(expression)?;
                has_entry_expression = true;
            }
        }
        if !has_entry_expression {
            emitter.emit_immediate(wsm_os_target::NIL);
        }
        if frame_bytes != 0 {
            emitter.line(&format!("    addq ${frame_bytes}, %rsp"));
        }
        emitter.line("    popq %r12");
        emitter.line("    ret");
        // Definition bodies are emitted out of line: reaching one requires
        // an explicit named call, never accidental entry-point fall-through.
        // Data-only defs (cml#8) have no body here -- already fully handled
        // by the startup initializer loop above.
        for expression in program {
            if let Ir::Def { value, .. } = expression {
                if matches!(value.as_ref(), Ir::Lambda { .. }) {
                    emitter.emit_ir(expression)?;
                }
            }
        }
        // A first-class named fixed-arity function reuses its ordinary named
        // body. The closure dispatcher supplies user arguments in %rsi..%r9
        // and the captured environment in %r10. Top-level named closures have
        // NIL environment, so the wrapper only restores the context register
        // before entering the regular `.Lfn_N`.
        let named_closure_wrappers: Vec<(usize, usize)> = emitter
            .named_closure_definitions
            .iter()
            .map(|(name, definition_id)| (*definition_id, emitter.functions[name]))
            .collect();
        for (definition_id, function_label) in named_closure_wrappers {
            emitter.line(&format!(".Lclosure_{definition_id}:"));
            emitter.line("    movq %r12, %rdi");
            emitter.line(&format!("    call .Lfn_{function_label}"));
            emitter.line("    ret");
        }
        emitter.line(".size wsm_entry, .-wsm_entry");
        let data_def_ids: Vec<usize> = emitter.data_defs.values().copied().collect();
        if !named_closure_ids.is_empty() || !data_def_ids.is_empty() {
            emitter.line(".section .bss");
            emitter.line(".align 8");
            for definition_id in named_closure_ids {
                emitter.line(&format!(".Lnamed_closure_word_{definition_id}:"));
                emitter.line("    .quad 0");
            }
            for definition_id in data_def_ids {
                emitter.line(&format!(".Ldata_word_{definition_id}:"));
                emitter.line("    .quad 0");
            }
        }
        emitter.line(".section .note.GNU-stack,\"\",@progbits");
        Ok(emitter.output)
    }

    /// Compile `(def name (params) body) (name initial_args...)` as a bounded
    /// stack loop rather than a recursive `call` chain.
    ///
    /// Structure:
    /// ```text
    /// wsm_entry:
    ///     pushq %r12
    ///     subq  $N, %rsp
    ///     movq  %rdi, %r12        ; save context
    ///     <evaluate initial arg → %rax>
    ///     movq  %rax, 0(%rsp)     ; store in param slot
    /// .Lloop:
    ///     <body: TailSelfCall → reload slot + jmp .Lloop>
    ///     ; fall-through on non-recursive branches → result in %rax
    ///     addq  $N, %rsp
    ///     popq  %r12
    ///     ret
    /// ```
    fn compile_tail_call_program(
        &self,
        name: &str,
        params: &[String],
        body: &Ir,
        initial_args: &[Ir],
    ) -> Result<String, CompileError> {
        // Preflight the body (admits TailSelfCall, rejects closures/general App).
        let mut symbol_names = BTreeSet::new();
        let mut def_arities = BTreeMap::new();
        let mut slots = 0_usize;
        let bindings: BTreeSet<String> = params.iter().cloned().collect();
        preflight_tail_body(
            body,
            &bindings,
            &mut symbol_names,
            &mut def_arities,
            &mut slots,
        )?;
        for arg in initial_args {
            preflight(arg, &mut symbol_names, &mut def_arities, &mut slots)?;
        }
        check_symbol_capacity(symbol_names.len() as u64)?;
        let symbols: BTreeMap<String, u64> = symbol_names
            .into_iter()
            .enumerate()
            .map(|(index, n)| (n, index as u64 + 1))
            .collect();

        // Reserve one slot per param + spill budget from body preflight.
        // Params live in slots 0..params.len(); body spills use the rest.
        let param_count = params.len();
        let total_slots = param_count + slots;
        let frame_bytes = total_slots.next_multiple_of(2) * 8;

        let mut env = BTreeMap::new();
        for (i, param) in params.iter().enumerate() {
            env.insert(param.clone(), i);
        }

        let mut emitter = Emitter {
            output: String::new(),
            symbols,
            env,
            // Body gets slots starting after the param slots.
            next_slot: param_count,
            next_label: 0,
            frame_bytes,
            next_gc_safepoint: 0,
            gc_emit_depth: 0,
            gc_structured_live_slots: Vec::new(),
            gc_structured_context_complete: false,
            closure_labels: BTreeMap::new(),
            named_closure_definitions: BTreeMap::new(),
            functions: BTreeMap::new(),
            function_arities: BTreeMap::new(),
            data_defs: BTreeMap::new(),
        };

        emitter.line(".text");
        emitter.line(".globl wsm_entry");
        emitter.line(".type wsm_entry, @function");
        emitter.line("wsm_entry:");
        emitter.line("    pushq %r12");
        emitter.line(&format!("    subq ${frame_bytes}, %rsp"));
        emitter.line("    movq %rdi, %r12");

        // Evaluate and store initial arguments into param slots.
        for (i, arg) in initial_args.iter().enumerate() {
            emitter.emit_ir(arg)?;
            emitter.line(&format!("    movq %rax, {}(%rsp)", Emitter::slot_offset(i)));
        }

        // Loop label — TailSelfCall jumps here.
        let loop_label = emitter.allocate_label();
        emitter.line(&format!(".Ltcloop_{}:", loop_label));

        // Emit the body with tail-call context.
        emitter.emit_tail_body(body, loop_label, param_count, None)?;

        // Epilogue (reached only from non-recursive branches).
        emitter.line(&format!("    addq ${frame_bytes}, %rsp"));
        emitter.line("    popq %r12");
        emitter.line("    ret");
        emitter.line(".size wsm_entry, .-wsm_entry");
        emitter.line(".section .note.GNU-stack,\"\",@progbits");

        let _ = name; // name used only for detection, not emitted
        Ok(emitter.output)
    }
}

fn contains_tail_self_call(ir: &Ir) -> bool {
    match ir {
        Ir::TailSelfCall { .. } => true,
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } | Ir::App { args, .. } => {
            args.iter().any(contains_tail_self_call)
        }
        Ir::Lambda { body, .. } | Ir::Def { value: body, .. } => contains_tail_self_call(body),
        Ir::Cond { branches } => branches
            .iter()
            .any(|(test, body)| contains_tail_self_call(test) || contains_tail_self_call(body)),
        Ir::Let { bindings, body } => {
            bindings
                .iter()
                .any(|(_, value)| contains_tail_self_call(value))
                || contains_tail_self_call(body)
        }
        _ => false,
    }
}

/// Find top-level bounded fixed-arity definitions that are read as values
/// rather than used only in direct call position. This keeps closure allocation demand tied to
/// an actual first-class use instead of allocating descriptors for every def.
fn collect_first_class_named_functions(
    program: &[Ir],
    def_arities: &BTreeMap<String, DefArity>,
) -> Result<BTreeSet<String>, CompileError> {
    let mut out = BTreeSet::new();
    let bound = BTreeSet::new();
    for expression in program {
        collect_first_class_named_refs(expression, def_arities, &bound, false, &mut out)?;
    }
    Ok(out)
}

fn collect_first_class_named_refs(
    ir: &Ir,
    def_arities: &BTreeMap<String, DefArity>,
    bound: &BTreeSet<String>,
    callee_position: bool,
    out: &mut BTreeSet<String>,
) -> Result<(), CompileError> {
    match ir {
        Ir::Var(name) if !callee_position && !bound.contains(name) => {
            // cml#8: a data-only def (Data) is an ordinary value read, not a
            // first-class-function-value attempt -- only a real function
            // entry is subject to the bounded fixed-arity gate below.
            match def_arities.get(name) {
                Some(DefArity::Fixed(arity)) if *arity <= 5 => {
                    out.insert(name.clone());
                }
                Some(DefArity::Fixed(_)) => {
                    return Err(CompileError::UnsupportedVariant(
                        "first-class named function (arity > 5)",
                    ));
                }
                Some(DefArity::Variadic { .. }) | Some(DefArity::AllRest) => {
                    return Err(CompileError::UnsupportedVariant(
                        "first-class named function (variadic)",
                    ));
                }
                Some(DefArity::Data) | None => {}
            }
        }
        Ir::Lambda { params, body } => {
            let mut nested = bound.clone();
            match params {
                Params::Fixed(names) => nested.extend(names.iter().cloned()),
                Params::Variadic { fixed, rest } => {
                    nested.extend(fixed.iter().cloned());
                    nested.insert(rest.clone());
                }
                Params::AllRest(rest) => {
                    nested.insert(rest.clone());
                }
            }
            collect_first_class_named_refs(body, def_arities, &nested, false, out)?;
        }
        Ir::App { func, args } => {
            collect_first_class_named_refs(func, def_arities, bound, true, out)?;
            for arg in args {
                collect_first_class_named_refs(arg, def_arities, bound, false, out)?;
            }
        }
        Ir::Cond { branches } => {
            for (test, body) in branches {
                collect_first_class_named_refs(test, def_arities, bound, false, out)?;
                collect_first_class_named_refs(body, def_arities, bound, false, out)?;
            }
        }
        Ir::Let { bindings, body } => {
            for (_, value) in bindings {
                collect_first_class_named_refs(value, def_arities, bound, false, out)?;
            }
            let mut nested = bound.clone();
            nested.extend(bindings.iter().map(|(name, _)| name.clone()));
            collect_first_class_named_refs(body, def_arities, &nested, false, out)?;
        }
        Ir::Def { value, .. } => {
            collect_first_class_named_refs(value, def_arities, bound, false, out)?;
        }
        Ir::Prim { args, .. } | Ir::MachinePrim { args, .. } | Ir::TailSelfCall { args } => {
            for arg in args {
                collect_first_class_named_refs(arg, def_arities, bound, false, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn preflight(
    ir: &Ir,
    symbols: &mut BTreeSet<String>,
    def_arities: &mut BTreeMap<String, DefArity>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    preflight_env(ir, &BTreeSet::new(), symbols, def_arities, slots)
}

fn preflight_env(
    ir: &Ir,
    bindings: &BTreeSet<String>,
    symbols: &mut BTreeSet<String>,
    def_arities: &mut BTreeMap<String, DefArity>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::Int(value) => {
            wsm_os_target::encode_fixnum(*value).ok_or(CompileError::FixnumOutOfRange(*value))?;
        }
        Ir::Float(_) => return Err(CompileError::UnsupportedVariant("Float")),
        Ir::Rational(_, _) => return Err(CompileError::UnsupportedVariant("Rational")),
        Ir::String(_) => return Err(CompileError::UnsupportedVariant("String")),
        Ir::Nil | Ir::True => {}
        Ir::Quote(value) => preflight_quoted(value, symbols, slots)?,
        Ir::Prim {
            op: PrimOp::CompilerMechanism(mechanism),
            args,
        } => {
            match *mechanism {
                RichCompilerMechanismRef::AtomPredicateD1
                | RichCompilerMechanismRef::AtomEqualityD1 => {
                    checked_current_predicate_mechanism(*mechanism, args.len())?;
                }
                _ => {
                    checked_current_compiler_runtime(*mechanism, args.len())?;
                }
            }
            for argument in args {
                preflight_env(argument, bindings, symbols, def_arities, slots)?;
            }
            return Ok(());
        }
        Ir::Prim {
            op: PrimOp::CompilerConditionalExactD1(mechanism),
            args,
        } => {
            checked_current_conditional_mechanism(*mechanism, args.len())?;
            for argument in args {
                preflight_env(argument, bindings, symbols, def_arities, slots)?;
            }
            return Ok(());
        }
        Ir::Prim { .. } => return Err(CompileError::UnsupportedVariant("Prim")),
        Ir::MachinePrim { op, args } => {
            let (name, expected) = machine_primitive_contract(*op)?;
            if args.len() != expected {
                return Err(CompileError::InvalidArity {
                    operation: name,
                    expected,
                    actual: args.len(),
                });
            }
            for argument in args {
                preflight_env(argument, bindings, symbols, def_arities, slots)?;
            }
            return Ok(());
        }
        Ir::Buffer(_) => return Err(CompileError::UnsupportedVariant("typed buffer")),
        Ir::Var(_) => {}
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() <= 5 => {
            let mut nested_bindings = bindings.clone();
            nested_bindings.extend(params.iter().cloned());
            preflight_lambda_body(body, &nested_bindings, symbols, slots)?;
        }
        Ir::Lambda { .. } => return Err(CompileError::UnsupportedVariant("lambda")),
        Ir::App { func, args } => {
            if let Ir::Sid(sid) = func.as_ref() {
                let Some((expected, _runtime)) = sid8_call_contract(*sid) else {
                    return Err(CompileError::UnimplementedSid8(*sid));
                };
                if let Some(expected) = expected {
                    if args.len() != expected {
                        return Err(CompileError::InvalidArity {
                            operation: "SID8",
                            expected,
                            actual: args.len(),
                        });
                    }
                }
                for argument in args {
                    preflight_env(argument, bindings, symbols, def_arities, slots)?;
                }
                return Ok(());
            }
            if let Some((operation, expected, _)) = platform_call_contract(func) {
                if args.len() != expected {
                    return Err(CompileError::InvalidArity {
                        operation,
                        expected,
                        actual: args.len(),
                    });
                }
                for argument in args {
                    preflight_env(argument, bindings, symbols, def_arities, slots)?;
                }
                return Ok(());
            }
            if let Ir::Lambda { params, body } = func.as_ref() {
                match params {
                    Params::Fixed(params) if params.len() == args.len() && params.len() <= 5 => {
                        for arg in args {
                            preflight_env(arg, bindings, symbols, def_arities, slots)?;
                        }
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.extend(params.iter().cloned());
                        return preflight_lambda_body(body, &nested_bindings, symbols, slots);
                    }
                    Params::Variadic { fixed, rest } if fixed.len() + 1 <= 5 => {
                        if args.len() < fixed.len() {
                            return Err(CompileError::InvalidArity {
                                operation: "variadic lambda",
                                expected: fixed.len(),
                                actual: args.len(),
                            });
                        }
                        for arg in args {
                            preflight_env(arg, bindings, symbols, def_arities, slots)?;
                        }
                        *slots += (args.len() - fixed.len()) * 2 + 2;
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.extend(fixed.iter().cloned());
                        nested_bindings.insert(rest.clone());
                        return preflight_lambda_body(body, &nested_bindings, symbols, slots);
                    }
                    Params::AllRest(rest) => {
                        for arg in args {
                            preflight_env(arg, bindings, symbols, def_arities, slots)?;
                        }
                        *slots += args.len() * 2 + 2;
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.insert(rest.clone());
                        return preflight_lambda_body(body, &nested_bindings, symbols, slots);
                    }
                    _ => {}
                }
            }
            if let Ir::Var(name) = func.as_ref() {
                match def_arities.get(name) {
                    Some(DefArity::Fixed(arity)) => {
                        let arity = *arity;
                        if args.len() != arity {
                            return Err(CompileError::DefArityMismatch {
                                name: name.clone(),
                                expected: arity,
                                actual: args.len(),
                            });
                        }
                        for argument in args {
                            preflight_env(argument, bindings, symbols, def_arities, slots)?;
                        }
                        return Ok(());
                    }
                    Some(DefArity::Variadic { fixed }) => {
                        let fixed = *fixed;
                        if args.len() < fixed {
                            return Err(CompileError::InvalidArity {
                                operation: "variadic function",
                                expected: fixed,
                                actual: args.len(),
                            });
                        }
                        if fixed + 1 > 5 {
                            return Err(CompileError::UnsupportedVariant(
                                "Def (too many params for variadic function)",
                            ));
                        }
                        for argument in args {
                            preflight_env(argument, bindings, symbols, def_arities, slots)?;
                        }
                        *slots += (args.len() - fixed) * 2 + 2;
                        return Ok(());
                    }
                    Some(DefArity::AllRest) => {
                        for argument in args {
                            preflight_env(argument, bindings, symbols, def_arities, slots)?;
                        }
                        *slots += args.len() * 2 + 2;
                        return Ok(());
                    }
                    Some(DefArity::Data) => {
                        // cml#8: name is a known data-only def, not callable.
                        return Err(CompileError::UnsupportedVariant(
                            "application of a data-only def",
                        ));
                    }
                    None => {}
                }
            }
            if args.len() > 5 {
                return Err(CompileError::UnsupportedVariant(
                    "application (more than 5 runtime arguments)",
                ));
            }
            preflight_env(func, bindings, symbols, def_arities, slots)?;
            for argument in args {
                preflight_env(argument, bindings, symbols, def_arities, slots)?;
            }
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight_env(test, bindings, symbols, def_arities, slots)?;
                preflight_env(expr, bindings, symbols, def_arities, slots)?;
            }
        }
        Ir::Let {
            bindings: let_bindings,
            body,
        } => {
            // Top-level `let` admits the same parallel-binding shape already
            // handled inside named-definition bodies: every value form is
            // preflight-checked in the enclosing environment, then the body.
            // Binding names are lexical, so a Var read of one is valid.
            for (_, value) in let_bindings {
                preflight_env(value, bindings, symbols, def_arities, slots)?;
            }
            let mut new_bindings = bindings.clone();
            new_bindings.extend(let_bindings.iter().map(|(name, _)| name.clone()));
            preflight_env(body, &new_bindings, symbols, def_arities, slots)?;
        }
        Ir::Def { name, value } => {
            match value.as_ref() {
                Ir::Lambda {
                    params: Params::Fixed(param_names),
                    body,
                } => {
                    let bindings: BTreeSet<String> = param_names.iter().cloned().collect();
                    def_arities.insert(name.clone(), DefArity::Fixed(param_names.len()));
                    preflight_def_body(body, &bindings, symbols, def_arities, slots)?;
                    symbols.insert(name.clone());
                    return Ok(());
                }
                Ir::Lambda {
                    params: Params::Variadic { fixed, rest },
                    body,
                } => {
                    if fixed.len() + 1 > 5 {
                        return Err(CompileError::UnsupportedVariant(
                            "Def (too many params for variadic function)",
                        ));
                    }
                    let mut bindings: BTreeSet<String> = fixed.iter().cloned().collect();
                    bindings.insert(rest.clone());
                    def_arities.insert(name.clone(), DefArity::Variadic { fixed: fixed.len() });
                    preflight_def_body(body, &bindings, symbols, def_arities, slots)?;
                    symbols.insert(name.clone());
                    return Ok(());
                }
                Ir::Lambda {
                    params: Params::AllRest(rest),
                    body,
                } => {
                    let mut bindings = BTreeSet::new();
                    bindings.insert(rest.clone());
                    def_arities.insert(name.clone(), DefArity::AllRest);
                    preflight_def_body(body, &bindings, symbols, def_arities, slots)?;
                    symbols.insert(name.clone());
                    return Ok(());
                }
                _ => {
                    // A top-level def whose value is not a lambda at all -- an
                    // ordinary data binding (cml#8). Evaluated once at wsm_entry
                    // startup into a dedicated word slot, the same evaluate-once-
                    // into-a-slot shape PR #7's closure descriptors already use,
                    // not a callable and not admitted as one. The value
                    // expression itself still goes through ordinary preflight
                    // (it may be a quoted literal, a primitive call, another
                    // Var, etc.) -- only forward references to a *later* data
                    // def are not handled by this first slice, since data-def
                    // initializers run in program order at startup.
                    preflight(value, symbols, def_arities, slots)?;
                    symbols.insert(name.clone());
                    return Ok(());
                }
            }
        }
        Ir::TailSelfCall { .. } => {
            return Err(CompileError::UnsupportedVariant(
                "TailSelfCall outside a tail-call program",
            ));
        }
        Ir::Sid(_) => {
            return Err(CompileError::UnsupportedVariant(
                "bare Sid value in x86 preflight",
            ));
        }
        Ir::Builtin(_) => {
            return Err(CompileError::UnsupportedVariant(
                "Builtin value in x86 preflight",
            ));
        }
        Ir::CondMatch { .. } => {
            return Err(CompileError::UnsupportedVariant(
                "CondMatch in x86 preflight",
            ));
        }
    }
    Ok(())
}

fn preflight_lambda_body(
    ir: &Ir,
    bindings: &BTreeSet<String>,
    symbols: &mut BTreeSet<String>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::Var(name) if bindings.contains(name) => Ok(()),
        Ir::Var(_) => Err(CompileError::UnsupportedVariant("unbound variable")),
        Ir::Int(value) => {
            wsm_os_target::encode_fixnum(*value).ok_or(CompileError::FixnumOutOfRange(*value))?;
            Ok(())
        }
        Ir::Nil | Ir::True => Ok(()),
        Ir::Quote(value) => preflight_quoted(value, symbols, slots),
        Ir::Prim {
            op: PrimOp::CompilerMechanism(mechanism),
            args,
        } => {
            match *mechanism {
                RichCompilerMechanismRef::AtomPredicateD1
                | RichCompilerMechanismRef::AtomEqualityD1 => {
                    checked_current_predicate_mechanism(*mechanism, args.len())?;
                }
                _ => {
                    checked_current_compiler_runtime(*mechanism, args.len())?;
                }
            }
            for argument in args {
                preflight_lambda_body(argument, bindings, symbols, slots)?;
            }
            Ok(())
        }
        Ir::Prim {
            op: PrimOp::CompilerConditionalExactD1(mechanism),
            args,
        } => {
            checked_current_conditional_mechanism(*mechanism, args.len())?;
            for argument in args {
                preflight_lambda_body(argument, bindings, symbols, slots)?;
            }
            Ok(())
        }
        Ir::Prim { .. } => Err(CompileError::UnsupportedVariant("Prim")),
        Ir::MachinePrim { op, args } => {
            let (name, expected) = machine_primitive_contract(*op)?;
            if args.len() != expected {
                return Err(CompileError::InvalidArity {
                    operation: name,
                    expected,
                    actual: args.len(),
                });
            }
            for argument in args {
                preflight_lambda_body(argument, bindings, symbols, slots)?;
            }
            Ok(())
        }
        Ir::Cond { branches } => {
            for (test, expression) in branches {
                preflight_lambda_body(test, bindings, symbols, slots)?;
                preflight_lambda_body(expression, bindings, symbols, slots)?;
            }
            Ok(())
        }
        Ir::App { func, args } => {
            if let Ir::Sid(sid) = func.as_ref() {
                let Some((expected, _runtime)) = sid8_call_contract(*sid) else {
                    return Err(CompileError::UnimplementedSid8(*sid));
                };
                if let Some(expected) = expected {
                    if args.len() != expected {
                        return Err(CompileError::InvalidArity {
                            operation: "SID8",
                            expected,
                            actual: args.len(),
                        });
                    }
                }
                for argument in args {
                    preflight_lambda_body(argument, bindings, symbols, slots)?;
                }
                return Ok(());
            }
            if let Some((operation, expected, _)) = platform_call_contract(func) {
                if !matches!(func.as_ref(), Ir::Var(name) if bindings.contains(name)) {
                    if args.len() != expected {
                        return Err(CompileError::InvalidArity {
                            operation,
                            expected,
                            actual: args.len(),
                        });
                    }
                    for argument in args {
                        preflight_lambda_body(argument, bindings, symbols, slots)?;
                    }
                    return Ok(());
                }
            }
            if let Ir::Lambda { params, body } = func.as_ref() {
                match params {
                    Params::Fixed(params) if params.len() == args.len() && params.len() <= 5 => {
                        for arg in args {
                            preflight_lambda_body(arg, bindings, symbols, slots)?;
                        }
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.extend(params.iter().cloned());
                        return preflight_lambda_body(body, &nested_bindings, symbols, slots);
                    }
                    Params::Variadic { fixed, rest } if fixed.len() + 1 <= 5 => {
                        if args.len() < fixed.len() {
                            return Err(CompileError::InvalidArity {
                                operation: "variadic lambda",
                                expected: fixed.len(),
                                actual: args.len(),
                            });
                        }
                        for arg in args {
                            preflight_lambda_body(arg, bindings, symbols, slots)?;
                        }
                        *slots += (args.len() - fixed.len()) * 2 + 2;
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.extend(fixed.iter().cloned());
                        nested_bindings.insert(rest.clone());
                        return preflight_lambda_body(body, &nested_bindings, symbols, slots);
                    }
                    Params::AllRest(rest) => {
                        for arg in args {
                            preflight_lambda_body(arg, bindings, symbols, slots)?;
                        }
                        *slots += args.len() * 2 + 2;
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.insert(rest.clone());
                        return preflight_lambda_body(body, &nested_bindings, symbols, slots);
                    }
                    _ => {}
                }
            }
            if args.len() > 5 {
                return Err(CompileError::UnsupportedVariant(
                    "application (more than 5 runtime arguments)",
                ));
            }
            preflight_lambda_body(func, bindings, symbols, slots)?;
            for argument in args {
                preflight_lambda_body(argument, bindings, symbols, slots)?;
            }
            Ok(())
        }
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() <= 5 => {
            let mut nested_bindings = bindings.clone();
            nested_bindings.extend(params.iter().cloned());
            preflight_lambda_body(body, &nested_bindings, symbols, slots)
        }
        Ir::Let {
            bindings: let_bindings,
            body,
        } => {
            let mut new_bindings = bindings.clone();
            for (name, val) in let_bindings {
                preflight_lambda_body(val, bindings, symbols, slots)?;
                new_bindings.insert(name.clone());
            }
            preflight_lambda_body(body, &new_bindings, symbols, slots)
        }
        _ => Err(CompileError::UnsupportedVariant("lambda body")),
    }
}

/// Like `preflight` but permits `TailSelfCall` nodes (the body of an
/// admitted `Def`). `App`, `Lambda`, `Def` and `Var` remain rejected.
fn preflight_tail_body(
    ir: &Ir,
    bindings: &BTreeSet<String>,
    symbols: &mut BTreeSet<String>,
    def_arities: &mut BTreeMap<String, DefArity>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::TailSelfCall { args } => {
            for arg in args {
                preflight_def_body(arg, bindings, symbols, def_arities, slots)?;
            }
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight_def_body(test, bindings, symbols, def_arities, slots)?;
                preflight_tail_body(expr, bindings, symbols, def_arities, slots)?;
            }
        }
        Ir::Let {
            bindings: let_bindings,
            body,
        } => {
            let mut nested_bindings = bindings.clone();
            for (name, val) in let_bindings {
                preflight_def_body(val, bindings, symbols, def_arities, slots)?;
                nested_bindings.insert(name.clone());
            }
            preflight_tail_body(body, &nested_bindings, symbols, def_arities, slots)?;
        }
        other => preflight_def_body(other, bindings, symbols, def_arities, slots)?,
    }
    Ok(())
}

/// Like `preflight` but permits `TailSelfCall` nodes and accepts the given
/// bindings as valid variables (for Def body preflight).
fn preflight_def_body(
    ir: &Ir,
    bindings: &BTreeSet<String>,
    symbols: &mut BTreeSet<String>,
    def_arities: &mut BTreeMap<String, DefArity>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::Var(name) if bindings.contains(name) => Ok(()),
        Ir::Var(name) => match def_arities.get(name) {
            Some(DefArity::Fixed(arity)) if *arity <= 5 => Ok(()),
            Some(DefArity::Fixed(_)) => Err(CompileError::UnsupportedVariant(
                "first-class named function (arity > 5)",
            )),
            Some(DefArity::Variadic { .. }) | Some(DefArity::AllRest) => Err(
                CompileError::UnsupportedVariant("first-class named function (variadic)"),
            ),
            // cml#8: a data-only def is an ordinary value read.
            Some(DefArity::Data) => Ok(()),
            None => Err(CompileError::UnsupportedVariant("unbound variable")),
        },
        // A Canon builtin identity (e.g. numeric `=`, SID 00011100) is a
        // legitimate first-class value inside a def body; the preflight
        // admits the identity, while actual machine support remains an emit
        // concern that fail-closes precisely (admitted-but-partial, #92).
        // #246: first-class callables now carry the exact Sid8, admitted here
        // the same way -- the emit layer still fail-closes on the standalone
        // word unless it can materialize an actual loadable value.
        Ir::Builtin(_) | Ir::Sid(_) => Ok(()),
        Ir::Int(value) => {
            wsm_os_target::encode_fixnum(*value).ok_or(CompileError::FixnumOutOfRange(*value))?;
            Ok(())
        }
        Ir::Nil | Ir::True => Ok(()),
        Ir::Quote(value) => preflight_quoted(value, symbols, slots),
        Ir::Prim {
            op: PrimOp::CompilerMechanism(mechanism),
            args,
        } => {
            match *mechanism {
                RichCompilerMechanismRef::AtomPredicateD1
                | RichCompilerMechanismRef::AtomEqualityD1 => {
                    checked_current_predicate_mechanism(*mechanism, args.len())?;
                }
                _ => {
                    checked_current_compiler_runtime(*mechanism, args.len())?;
                }
            }
            for argument in args {
                preflight_def_body(argument, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::Prim {
            op: PrimOp::CompilerConditionalExactD1(mechanism),
            args,
        } => {
            checked_current_conditional_mechanism(*mechanism, args.len())?;
            for argument in args {
                preflight_def_body(argument, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::Prim { .. } => Err(CompileError::UnsupportedVariant("Prim")),
        Ir::MachinePrim { op, args } => {
            let (name, expected) = machine_primitive_contract(*op)?;
            if args.len() != expected {
                return Err(CompileError::InvalidArity {
                    operation: name,
                    expected,
                    actual: args.len(),
                });
            }
            for argument in args {
                preflight_def_body(argument, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::App { func, args } => {
            if let Ir::Sid(sid) = func.as_ref() {
                if let Some((expected, _runtime)) = sid8_call_contract(*sid) {
                    if let Some(expected) = expected {
                        if args.len() != expected {
                            return Err(CompileError::InvalidArity {
                                operation: "SID8",
                                expected,
                                actual: args.len(),
                            });
                        }
                    }
                    for argument in args {
                        preflight_def_body(argument, bindings, symbols, def_arities, slots)?;
                    }
                    return Ok(());
                }
                // Typed user-def call (#238): the definition is registered
                // under this same 8-bit pattern (key_definition_by_sid). Treat
                // the pattern as the def's name and reuse the exact named-call
                // validation (arity, slots, errors).
                let key = sid.to_string();
                if def_arities.contains_key(&key) {
                    let named = Ir::App {
                        func: Box::new(Ir::Var(key)),
                        args: args.to_vec(),
                    };
                    return preflight_def_body(&named, bindings, symbols, def_arities, slots);
                }
                return Err(CompileError::UnimplementedSid8(*sid));
            }
            if let Some((operation, expected, _)) = platform_call_contract(func) {
                if !matches!(func.as_ref(), Ir::Var(name) if bindings.contains(name)) {
                    if args.len() != expected {
                        return Err(CompileError::InvalidArity {
                            operation,
                            expected,
                            actual: args.len(),
                        });
                    }
                    for argument in args {
                        preflight_def_body(argument, bindings, symbols, def_arities, slots)?;
                    }
                    return Ok(());
                }
            }
            if let Ir::Var(name) = func.as_ref() {
                if !bindings.contains(name) {
                    match def_arities.get(name) {
                        Some(DefArity::Fixed(arity)) => {
                            let arity = *arity;
                            if args.len() != arity {
                                return Err(CompileError::DefArityMismatch {
                                    name: name.clone(),
                                    expected: arity,
                                    actual: args.len(),
                                });
                            }
                            for argument in args {
                                preflight_def_body(
                                    argument,
                                    bindings,
                                    symbols,
                                    def_arities,
                                    slots,
                                )?;
                            }
                            return Ok(());
                        }
                        Some(DefArity::Variadic { fixed }) => {
                            let fixed = *fixed;
                            if args.len() < fixed {
                                return Err(CompileError::InvalidArity {
                                    operation: "variadic function",
                                    expected: fixed,
                                    actual: args.len(),
                                });
                            }
                            if fixed + 1 > 5 {
                                return Err(CompileError::UnsupportedVariant(
                                    "Def (too many params for variadic function)",
                                ));
                            }
                            for argument in args {
                                preflight_def_body(
                                    argument,
                                    bindings,
                                    symbols,
                                    def_arities,
                                    slots,
                                )?;
                            }
                            *slots += (args.len() - fixed) * 2 + 2;
                            return Ok(());
                        }
                        Some(DefArity::AllRest) => {
                            for argument in args {
                                preflight_def_body(
                                    argument,
                                    bindings,
                                    symbols,
                                    def_arities,
                                    slots,
                                )?;
                            }
                            *slots += args.len() * 2 + 2;
                            return Ok(());
                        }
                        Some(DefArity::Data) => {
                            return Err(CompileError::UnsupportedVariant(
                                "application of a data-only def",
                            ));
                        }
                        None => {}
                    }
                }
            }
            if let Ir::Lambda { params, body } = func.as_ref() {
                match params {
                    Params::Fixed(params) if params.len() == args.len() && params.len() <= 5 => {
                        for arg in args {
                            preflight_def_body(arg, bindings, symbols, def_arities, slots)?;
                        }
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.extend(params.iter().cloned());
                        return preflight_def_body(
                            body,
                            &nested_bindings,
                            symbols,
                            def_arities,
                            slots,
                        );
                    }
                    Params::Variadic { fixed, rest } if fixed.len() + 1 <= 5 => {
                        if args.len() < fixed.len() {
                            return Err(CompileError::InvalidArity {
                                operation: "variadic lambda",
                                expected: fixed.len(),
                                actual: args.len(),
                            });
                        }
                        for arg in args {
                            preflight_def_body(arg, bindings, symbols, def_arities, slots)?;
                        }
                        *slots += (args.len() - fixed.len()) * 2 + 2;
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.extend(fixed.iter().cloned());
                        nested_bindings.insert(rest.clone());
                        return preflight_def_body(
                            body,
                            &nested_bindings,
                            symbols,
                            def_arities,
                            slots,
                        );
                    }
                    Params::AllRest(rest) => {
                        for arg in args {
                            preflight_def_body(arg, bindings, symbols, def_arities, slots)?;
                        }
                        *slots += args.len() * 2 + 2;
                        let mut nested_bindings = bindings.clone();
                        nested_bindings.insert(rest.clone());
                        return preflight_def_body(
                            body,
                            &nested_bindings,
                            symbols,
                            def_arities,
                            slots,
                        );
                    }
                    _ => {}
                }
            }
            if args.len() > 5 {
                return Err(CompileError::UnsupportedVariant(
                    "application (more than 5 runtime arguments)",
                ));
            }
            preflight_def_body(func, bindings, symbols, def_arities, slots)?;
            for argument in args {
                preflight_def_body(argument, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() <= 5 => {
            let mut nested_bindings = bindings.clone();
            nested_bindings.extend(params.iter().cloned());
            preflight_def_body(body, &nested_bindings, symbols, def_arities, slots)
        }
        Ir::TailSelfCall { args } => {
            for arg in args {
                preflight_def_body(arg, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight_def_body(test, bindings, symbols, def_arities, slots)?;
                preflight_def_body(expr, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::CondMatch { branches } => {
            for (query, expected, body) in branches {
                preflight_def_body(query, bindings, symbols, def_arities, slots)?;
                preflight_quoted(expected, symbols, slots)?;
                preflight_def_body(body, bindings, symbols, def_arities, slots)?;
            }
            Ok(())
        }
        Ir::Let {
            bindings: let_bindings,
            body,
        } => {
            let mut new_bindings = bindings.clone();
            for (name, val) in let_bindings {
                preflight_def_body(val, bindings, symbols, def_arities, slots)?;
                new_bindings.insert(name.clone());
            }
            preflight_def_body(body, &new_bindings, symbols, def_arities, slots)
        }
        _ => Err(CompileError::UnsupportedVariant("def body")),
    }
}

fn preflight_quoted(
    quoted: &Quoted,
    symbols: &mut BTreeSet<String>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match quoted {
        Quoted::Int(value) => {
            wsm_os_target::encode_fixnum(*value).ok_or(CompileError::FixnumOutOfRange(*value))?;
        }
        // cml#13: preserve my-lisp's exact quoted-symbol data identity --
        // `original`, not the uppercased target-identifier convention other
        // backends key on. `radio` and `RADIO` must stay distinct symbols.
        Quoted::Sym { original, .. } => {
            symbols.insert(original.clone());
        }
        // The wsm-os target ABI has image-local symbols but no string
        // representation. Do not silently collapse a persistent WSM FS
        // string (for example a binding name) into a symbol.
        Quoted::Str(_) => {
            return Err(CompileError::UnsupportedVariant(
                "quoted string (target ABI has no string representation)",
            ));
        }
        Quoted::Nil => {}
        Quoted::List(values) => {
            for value in values {
                preflight_quoted(value, symbols, slots)?;
            }
        }
        Quoted::DottedList(values, tail) => {
            for value in values {
                preflight_quoted(value, symbols, slots)?;
            }
            preflight_quoted(tail, symbols, slots)?;
        }
        _ => {
            return Err(CompileError::UnsupportedVariant(
                "unsupported Quoted node in x86 preflight",
            ));
        }
    }
    Ok(())
}

fn sid8_call_contract(sid: sens::Sid8) -> Option<(Option<usize>, &'static str)> {
    if sid == sens::sid!(00000010) {
        Some((Some(1), "wsm_atom"))
    } else if sid == sens::sid!(00000011)
        || sid == sens::sid!(00011100)
        || sid == sens::sid!(00100010)
    {
        // `eq` (00000011), numeric `=` (00011100) and `equal?` (00100010)
        // are all word equality on the target representation.
        Some((Some(2), "wsm_eq"))
    } else if sid == sens::sid!(00000100) {
        Some((Some(2), "wsm_cons"))
    } else if sid == sens::sid!(00000101) {
        Some((Some(1), "wsm_car"))
    } else if sid == sens::sid!(00000110) {
        Some((Some(1), "wsm_cdr"))
    } else if sid == sens::sid!(00100111) {
        Some((None, "wsm_cons"))
    } else if sid == sens::sid!(00001100)
        || sid == sens::sid!(00001101)
        || sid == sens::sid!(00001110)
        || sid == sens::sid!(00010011)
        || sid == sens::sid!(00011010)
        || sid == sens::sid!(00011101)
        || sid == sens::sid!(00011110)
        || sid == sens::sid!(00010100)
    {
        // Inline arithmetic / mod / exact-Q / quotient: arity 2, no runtime name.
        Some((Some(2), ""))
    } else if sid == sens::sid!(00110011)
        || sid == sens::sid!(00110100)
        || sid == sens::sid!(00110101)
        || sid == sens::sid!(00110110)
    {
        // Composed car/cdr accessors: arity 1, no runtime name.
        Some((Some(1), ""))
    } else {
        None
    }
}

fn machine_primitive_contract(operation: MachineOp) -> Result<(&'static str, usize), CompileError> {
    match operation {
        MachineOp::Rdtsc => Ok(("rdtsc", 0)),
        MachineOp::PciConfigCapability => Ok(("pci-config-capability", 0)),
        MachineOp::PciConfigRead16 => Ok(("pci-config-read16", 5)),
        MachineOp::MmioCapability => Ok(("mmio-capability", 0)),
        MachineOp::MmioRead32 => Ok(("mmio-read32", 2)),
        MachineOp::MmioWrite32 => Ok(("mmio-write32", 3)),
    }
}

fn platform_call_contract(func: &Ir) -> Option<(&'static str, usize, &'static str)> {
    let Ir::Var(name) = func else {
        return None;
    };
    match name.as_str() {
        "PCI-CONFIG-CAPABILITY" => Some(("pci-config-capability", 0, "wsm_pci_config_capability")),
        "PCI-CONFIG-READ16" => Some(("pci-config-read16", 5, "wsm_pci_config_read16")),
        "MMIO-CAPABILITY" => Some(("mmio-capability", 0, "wsm_mmio_capability")),
        "MMIO-READ32" => Some(("mmio-read32", 2, "wsm_mmio_read32")),
        "MMIO-WRITE32" => Some(("mmio-write32", 3, "wsm_mmio_write32")),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum X86ArithmeticKind {
    Add,
    Sub,
    Mul,
}

struct Emitter {
    output: String,
    symbols: BTreeMap<String, u64>,
    env: BTreeMap<String, usize>,
    next_slot: usize,
    next_label: usize,
    // Research-only GC root certificate metadata. These fields affect emitted
    // comments only; they are not a runtime ABI or language semantics.
    frame_bytes: usize,
    next_gc_safepoint: usize,
    // #417 research-only structured liveness for the current native frame.
    // This state is used only to emit fail-closed root-map comments.
    gc_emit_depth: usize,
    gc_structured_live_slots: Vec<usize>,
    gc_structured_context_complete: bool,
    // definition_id -> fixed arity. Runtime closure objects stay unchanged
    // (definition_id + environment); arity is compiler-owned dispatch metadata
    // used to make dynamic calls fail closed on a mismatched call shape.
    closure_labels: BTreeMap<usize, usize>,
    named_closure_definitions: BTreeMap<String, usize>,
    functions: BTreeMap<String, usize>,
    function_arities: BTreeMap<String, DefArity>,
    /// Top-level data-only defs (cml#8): name -> word-slot id, evaluated
    /// once at wsm_entry startup, mirroring named_closure_definitions'
    /// evaluate-once-into-a-slot shape but for ordinary values instead of
    /// closure descriptors.
    data_defs: BTreeMap<String, usize>,
}

impl Emitter {
    fn line(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }

    fn allocate_slot(&mut self) -> usize {
        let slot = self.next_slot;
        self.next_slot += 1;
        slot
    }

    fn allocate_label(&mut self) -> usize {
        let label = self.next_label;
        self.next_label += 1;
        label
    }

    fn slot_offset(slot: usize) -> usize {
        slot * 8
    }

    /// cml#413: emit a research-only precise-root certificate as assembler
    /// comments. The certificate names rewriteable native locations; it does
    /// not alter machine code, runtime ABI, or SENS semantics.
    fn emit_gc_root_certificate(
        &mut self,
        kind: &str,
        allocator: &str,
        stack_slots: &[usize],
        register_roots: &[&str],
    ) -> String {
        let id = self.next_gc_safepoint;
        self.next_gc_safepoint += 1;
        let return_label = format!(".Lgc_return_{id}");

        let mut stack_slots = stack_slots.to_vec();
        stack_slots.sort_unstable();
        stack_slots.dedup();

        self.line(&format!(
            "    # GC_SAFEPOINT id={id} kind={kind} allocator={allocator} frame={} return_label={return_label}",
            self.frame_bytes
        ));
        for slot in stack_slots {
            self.line(&format!(
                "    # GC_STACK_ROOT id={id} offset={}",
                Self::slot_offset(slot)
            ));
        }
        for register in register_roots {
            self.line(&format!("    # GC_REGISTER_ROOT id={id} reg={register}"));
        }
        return_label
    }

    fn emit_ir(&mut self, ir: &Ir) -> Result<(), CompileError> {
        self.gc_emit_depth += 1;
        let result = self.emit_ir_inner(ir);
        self.gc_emit_depth -= 1;
        result
    }

    fn emit_ir_inner(&mut self, ir: &Ir) -> Result<(), CompileError> {
        match ir {
            Ir::Sid(_) => Err(CompileError::UnsupportedVariant("standalone SID8 value")),
            Ir::Int(value) => {
                let word = wsm_os_target::encode_fixnum(*value)
                    .ok_or(CompileError::FixnumOutOfRange(*value))?;
                self.emit_immediate(word);
                Ok(())
            }
            Ir::Float(_) => Err(CompileError::UnsupportedVariant("Float")),
            Ir::Rational(_, _) => Err(CompileError::UnsupportedVariant("Rational")),
            Ir::String(_) => Err(CompileError::UnsupportedVariant("String")),
            Ir::Buffer(_) => Err(CompileError::UnsupportedVariant("Buffer")),
            Ir::Nil => {
                self.emit_immediate(wsm_os_target::NIL);
                Ok(())
            }
            Ir::True => {
                self.emit_immediate(CANONICAL_T);
                Ok(())
            }
            Ir::Var(name) => {
                if let Some(&slot) = self.env.get(name) {
                    self.line(&format!("    movq {}(%rsp), %rax", Self::slot_offset(slot)));
                    Ok(())
                } else if let Some(&definition_id) = self.named_closure_definitions.get(name) {
                    self.line(&format!(
                        "    movq .Lnamed_closure_word_{definition_id}(%rip), %rax"
                    ));
                    Ok(())
                } else if let Some(&definition_id) = self.data_defs.get(name) {
                    self.line(&format!("    movq .Ldata_word_{definition_id}(%rip), %rax"));
                    Ok(())
                } else {
                    Err(CompileError::UnsupportedVariant("Var (unbound)"))
                }
            }
            Ir::Builtin(_) => Err(CompileError::UnsupportedVariant("Builtin")),
            Ir::Quote(value) => self.emit_quoted(value),
            Ir::Lambda {
                params: Params::Fixed(params),
                body,
            } if params.len() <= 5 => self.emit_fixed_arity_closure_value(params, body),
            Ir::Lambda {
                params: Params::Fixed(_),
                ..
            } => Err(CompileError::UnsupportedVariant(
                "Lambda (fixed, arity > 5)",
            )),
            Ir::Lambda {
                params: Params::Variadic { .. },
                ..
            }
            | Ir::Lambda {
                params: Params::AllRest(_),
                ..
            } => Err(CompileError::UnsupportedVariant(
                "Lambda (variadic/all-rest)",
            )),
            Ir::App { func, args } => {
                if let Ir::Sid(sid) = func.as_ref() {
                    if sid8_call_contract(*sid).is_some() {
                        return self.emit_sid8_call(*sid, args);
                    }
                    // Typed user-def call (#238): the def is registered under
                    // this same 8-bit pattern (key_definition_by_sid). Dispatch
                    // it exactly like a named function whose name is the
                    // pattern; the 8-bit pattern is that name in the registry.
                    let key = sid.to_string();
                    if self.functions.contains_key(&key) || self.data_defs.contains_key(&key) {
                        let named = Ir::Var(key.clone());
                        return self.emit_named_def_call(&named, &key, args);
                    }
                    return Err(CompileError::UnimplementedSid8(*sid));
                }
                if platform_call_contract(func).is_some()
                    && !matches!(func.as_ref(), Ir::Var(name) if self.env.contains_key(name))
                {
                    self.emit_platform_call(func, args)
                } else if let Ir::Lambda { params, body } = func.as_ref() {
                    match params {
                        Params::Fixed(params)
                            if params.len() == args.len() && params.len() <= 5 =>
                        {
                            self.emit_direct_lambda_call(params, body, args)
                        }
                        Params::Variadic { fixed, rest }
                            if args.len() >= fixed.len() && fixed.len() + 1 <= 5 =>
                        {
                            self.emit_direct_variadic_lambda_call(fixed, rest, body, args)
                        }
                        Params::AllRest(rest) => {
                            self.emit_direct_variadic_lambda_call(&[], rest, body, args)
                        }
                        _ => Err(CompileError::UnsupportedVariant(
                            "App (multi-arg or non-lambda)",
                        )),
                    }
                } else if let Ir::Var(name) = func.as_ref() {
                    // Call a named function (admitted via Def), or a typed
                    // user-def whose registry name is an 8-bit pattern.
                    self.emit_named_def_call(func, name, args)
                } else {
                    self.emit_fixed_arity_closure_call(func, args)
                }
            }
            Ir::Cond { branches } => self.emit_cond(branches),
            Ir::CondMatch { branches } => self.emit_cond_match(branches),
            Ir::Let { bindings, body } => {
                // Top-level `let` binds in parallel: evaluate every value in
                // the enclosing environment, then install the name->slot
                // bindings for the body only. Restore the environment
                // afterwards so later top-level forms do not observe
                // lexical bindings.
                let saved_env = self.env.clone();
                let body_bindings: Vec<(String, usize)> = bindings
                    .iter()
                    .map(|(name, value)| {
                        self.emit_ir(value)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        Ok((name.clone(), slot))
                    })
                    .collect::<Result<_, CompileError>>()?;
                for (name, slot) in &body_bindings {
                    self.env.insert(name.clone(), *slot);
                }
                let result = self.emit_ir(body);
                self.env = saved_env;
                result
            }
            Ir::Def { name, value } => {
                match value.as_ref() {
                    Ir::Lambda {
                        params: Params::Fixed(param_names),
                        body,
                    } => {
                        let label = *self
                            .functions
                            .get(name)
                            .ok_or(CompileError::UnsupportedVariant("Def (not top-level)"))?;

                        // Named functions own an aligned native frame. `%rdi` is
                        // the runtime context; user arguments begin in `%rsi`.
                        let mut ignored_symbols = BTreeSet::new();
                        let mut ignored_arities = self.function_arities.clone();
                        let mut body_slots = 0_usize;
                        let bindings: BTreeSet<String> = param_names.iter().cloned().collect();
                        preflight_def_body(
                            body,
                            &bindings,
                            &mut ignored_symbols,
                            &mut ignored_arities,
                            &mut body_slots,
                        )?;
                        let required_slots = param_names.len() + body_slots;
                        let frame_slots = required_slots.max(1) | 1;
                        let frame_bytes = frame_slots * 8;
                        self.line(&format!(".Lfn_{label}:"));
                        self.line(&format!("    subq ${frame_bytes}, %rsp"));
                        let mut param_env = BTreeMap::new();
                        let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
                        for (i, param) in param_names.iter().enumerate() {
                            if i < regs.len() {
                                self.line(&format!(
                                    "    movq {}, {}(%rsp)",
                                    regs[i],
                                    Self::slot_offset(i)
                                ));
                            } else {
                                return Err(CompileError::UnsupportedVariant(
                                    "Def (too many params)",
                                ));
                            }
                            param_env.insert(param.clone(), i);
                        }
                        self.line(&format!(".Ltcloop_{label}:"));
                        let old_env = std::mem::replace(&mut self.env, param_env);
                        let old_next_slot = self.next_slot;
                        self.next_slot = param_names.len();
                        let old_frame_bytes = self.frame_bytes;
                        self.frame_bytes = frame_bytes;
                        let body_result = self.emit_tail_body(body, label, param_names.len(), None);
                        self.frame_bytes = old_frame_bytes;
                        body_result?;
                        self.env = old_env;
                        self.next_slot = old_next_slot;
                        self.line(&format!("    addq ${frame_bytes}, %rsp"));
                        self.line("    ret");
                        Ok(())
                    }
                    Ir::Lambda {
                        params: Params::Variadic { fixed, rest },
                        body,
                    } => {
                        let label = *self
                            .functions
                            .get(name)
                            .ok_or(CompileError::UnsupportedVariant("Def (not top-level)"))?;
                        let mut all_params = fixed.clone();
                        all_params.push(rest.clone());
                        if all_params.len() > 5 {
                            return Err(CompileError::UnsupportedVariant("Def (too many params)"));
                        }

                        let mut ignored_symbols = BTreeSet::new();
                        let mut ignored_arities = self.function_arities.clone();
                        let mut body_slots = 0_usize;
                        let bindings: BTreeSet<String> = all_params.iter().cloned().collect();
                        preflight_def_body(
                            body,
                            &bindings,
                            &mut ignored_symbols,
                            &mut ignored_arities,
                            &mut body_slots,
                        )?;
                        let required_slots = all_params.len() + body_slots;
                        let frame_slots = required_slots.max(1) | 1;
                        let frame_bytes = frame_slots * 8;
                        self.line(&format!(".Lfn_{label}:"));
                        self.line(&format!("    subq ${frame_bytes}, %rsp"));
                        let mut param_env = BTreeMap::new();
                        let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
                        for (i, param) in all_params.iter().enumerate() {
                            self.line(&format!(
                                "    movq {}, {}(%rsp)",
                                regs[i],
                                Self::slot_offset(i)
                            ));
                            param_env.insert(param.clone(), i);
                        }
                        self.line(&format!(".Ltcloop_{label}:"));
                        let old_env = std::mem::replace(&mut self.env, param_env);
                        let old_next_slot = self.next_slot;
                        self.next_slot = all_params.len();
                        let old_frame_bytes = self.frame_bytes;
                        self.frame_bytes = frame_bytes;
                        let body_result =
                            self.emit_tail_body(body, label, all_params.len(), Some(fixed.len()));
                        self.frame_bytes = old_frame_bytes;
                        body_result?;
                        self.env = old_env;
                        self.next_slot = old_next_slot;
                        self.line(&format!("    addq ${frame_bytes}, %rsp"));
                        self.line("    ret");
                        Ok(())
                    }
                    Ir::Lambda {
                        params: Params::AllRest(rest),
                        body,
                    } => {
                        let label = *self
                            .functions
                            .get(name)
                            .ok_or(CompileError::UnsupportedVariant("Def (not top-level)"))?;

                        let mut ignored_symbols = BTreeSet::new();
                        let mut ignored_arities = self.function_arities.clone();
                        let mut body_slots = 0_usize;
                        let bindings: BTreeSet<String> = BTreeSet::from([rest.clone()]);
                        preflight_def_body(
                            body,
                            &bindings,
                            &mut ignored_symbols,
                            &mut ignored_arities,
                            &mut body_slots,
                        )?;
                        let required_slots = 1 + body_slots;
                        let frame_slots = required_slots.max(1) | 1;
                        let frame_bytes = frame_slots * 8;
                        self.line(&format!(".Lfn_{label}:"));
                        self.line(&format!("    subq ${frame_bytes}, %rsp"));
                        self.line(&format!("    movq %rsi, {}(%rsp)", Self::slot_offset(0)));
                        let mut param_env = BTreeMap::new();
                        param_env.insert(rest.clone(), 0);
                        self.line(&format!(".Ltcloop_{label}:"));
                        let old_env = std::mem::replace(&mut self.env, param_env);
                        let old_next_slot = self.next_slot;
                        self.next_slot = 1;
                        let old_frame_bytes = self.frame_bytes;
                        self.frame_bytes = frame_bytes;
                        let body_result = self.emit_tail_body(body, label, 1, Some(0));
                        self.frame_bytes = old_frame_bytes;
                        body_result?;
                        self.env = old_env;
                        self.next_slot = old_next_slot;
                        self.line(&format!("    addq ${frame_bytes}, %rsp"));
                        self.line("    ret");
                        Ok(())
                    }
                    _ => Ok(()),
                }
            }
            Ir::Prim {
                op: PrimOp::CompilerMechanism(mechanism),
                args,
            } => match *mechanism {
                RichCompilerMechanismRef::AtomPredicateD1 => self.emit_current_atom_d1(args),
                RichCompilerMechanismRef::AtomEqualityD1 => self.emit_current_eq_d1(args),
                _ => {
                    let runtime = checked_current_compiler_runtime(*mechanism, args.len())?;
                    self.emit_runtime_call_with_structured_args(args, runtime)
                }
            },
            Ir::Prim {
                op: PrimOp::CompilerConditionalExactD1(mechanism),
                args,
            } => {
                checked_current_conditional_mechanism(*mechanism, args.len())?;
                self.emit_current_conditional_d1(args)
            }
            Ir::Prim { .. } => Err(CompileError::UnsupportedVariant("Prim")),
            Ir::MachinePrim { op, args } => self.emit_machine_primitive(*op, args),
            Ir::TailSelfCall { .. } => Err(CompileError::UnsupportedVariant("TailSelfCall")),
        }
    }

    fn emit_immediate(&mut self, word: u64) {
        self.line(&format!("    movabsq ${word}, %rax"));
    }

    fn emit_predicate_bit_runtime(&mut self, bit: u8) {
        debug_assert!(bit <= 1);
        self.line("    movq %r12, %rdi");
        self.line(&format!("    call wsm_predicate_bit_{bit}"));
    }

    fn emit_current_atom_d1(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        checked_current_predicate_mechanism(RichCompilerMechanismRef::AtomPredicateD1, args.len())?;
        self.emit_ir(&args[0])?;

        let no_label = self.allocate_label();
        let end_label = self.allocate_label();
        self.line("    movq %rax, %rcx");
        self.line(&format!("    andq ${}, %rcx", wsm_os_target::TAG_MASK));
        self.line(&format!(
            "    cmpq ${}, %rcx",
            wsm_os_target::Tag::Cons as u64
        ));
        self.line(&format!("    je .Lcurrent_atom_d1_no_{no_label}"));
        self.emit_predicate_bit_runtime(1);
        self.line(&format!("    jmp .Lcurrent_atom_d1_end_{end_label}"));
        self.line(&format!(".Lcurrent_atom_d1_no_{no_label}:"));
        self.emit_predicate_bit_runtime(0);
        self.line(&format!(".Lcurrent_atom_d1_end_{end_label}:"));
        Ok(())
    }

    fn emit_current_eq_d1(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        checked_current_predicate_mechanism(RichCompilerMechanismRef::AtomEqualityD1, args.len())?;

        self.emit_ir(&args[0])?;
        let left_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(left_slot)
        ));

        self.emit_ir(&args[1])?;
        let right_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(right_slot)
        ));

        let empty_label = self.allocate_label();
        let no_label = self.allocate_label();
        let end_label = self.allocate_label();

        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(left_slot)
        ));
        self.line(&format!("    andq ${}, %rcx", wsm_os_target::TAG_MASK));
        self.line(&format!(
            "    cmpq ${}, %rcx",
            wsm_os_target::Tag::Cons as u64
        ));
        self.line(&format!("    je .Lcurrent_eq_d1_empty_{empty_label}"));

        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(right_slot)
        ));
        self.line(&format!("    andq ${}, %rcx", wsm_os_target::TAG_MASK));
        self.line(&format!(
            "    cmpq ${}, %rcx",
            wsm_os_target::Tag::Cons as u64
        ));
        self.line(&format!("    je .Lcurrent_eq_d1_empty_{empty_label}"));

        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(left_slot)
        ));
        self.line(&format!(
            "    cmpq {}(%rsp), %rcx",
            Self::slot_offset(right_slot)
        ));
        self.line(&format!("    jne .Lcurrent_eq_d1_no_{no_label}"));
        self.emit_predicate_bit_runtime(1);
        self.line(&format!("    jmp .Lcurrent_eq_d1_end_{end_label}"));

        self.line(&format!(".Lcurrent_eq_d1_no_{no_label}:"));
        self.emit_predicate_bit_runtime(0);
        self.line(&format!("    jmp .Lcurrent_eq_d1_end_{end_label}"));

        self.line(&format!(".Lcurrent_eq_d1_empty_{empty_label}:"));
        self.emit_immediate(wsm_os_target::NIL);
        self.line(&format!(".Lcurrent_eq_d1_end_{end_label}:"));
        Ok(())
    }

    fn emit_current_conditional_d1(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        checked_current_conditional_mechanism(RichCompilerMechanismRef::ConditionalD1, args.len())?;
        let end_label = self.allocate_label();

        for pair in args.chunks_exact(2) {
            let next_label = self.allocate_label();
            self.emit_ir(&pair[0])?;

            // Contract 11.8 / sens#4395: every test-position value must
            // be exact D1. Structural EMPTY is only the exhaustion result,
            // never an admitted predicate answer. The target runtime validates
            // BoxedKind::PredicateBit and fails closed on EMPTY, Fixnum 0/1,
            // Symbol(t), legacy True, or another boxed kind.
            self.line("    movq %rax, %rsi");
            self.line("    movq %r12, %rdi");
            self.line("    call wsm_predicate_bit_bits");
            self.line("    testq %rax, %rax");
            self.line(&format!("    je .Lcurrent_cond_d1_next_{next_label}"));

            self.emit_ir(&pair[1])?;
            self.line(&format!("    jmp .Lcurrent_cond_d1_end_{end_label}"));
            self.line(&format!(".Lcurrent_cond_d1_next_{next_label}:"));
        }

        self.emit_immediate(wsm_os_target::NIL);
        self.line(&format!(".Lcurrent_cond_d1_end_{end_label}:"));
        Ok(())
    }

    fn emit_runtime_call_with_structured_args(
        &mut self,
        args: &[Ir],
        runtime: &str,
    ) -> Result<(), CompileError> {
        // #417: bounded structured root state. A top-level runtime call in an
        // empty lexical environment may prove its own argument-evaluation
        // lifetime. Nested direct wsm_cons calls inherit that proof. More
        // complex nested forms fail closed by temporarily suspending
        // completeness rather than publishing a partial root map.
        let live_checkpoint = self.gc_structured_live_slots.len();
        let previous_complete = self.gc_structured_context_complete;
        if !previous_complete && self.gc_emit_depth == 1 && self.env.is_empty() {
            self.gc_structured_context_complete = true;
        }

        let result = (|| {
            let mut slots = Vec::with_capacity(args.len());
            for argument in args {
                let direct_nested_gc_proven_shape = match argument {
                    Ir::App { func, .. } => {
                        if let Ir::Sid(sid) = func.as_ref() {
                            matches!(sid8_call_contract(*sid), Some((Some(2), "wsm_cons")))
                                || *sid == sens::sid!(00100111)
                        } else {
                            matches!(platform_call_contract(func), Some((_, _, "wsm_cons")))
                                && !matches!(
                                    func.as_ref(),
                                    Ir::Var(name) if self.env.contains_key(name)
                                )
                        }
                    }
                    Ir::Quote(Quoted::List(_) | Quoted::DottedList(_, _)) => true,
                    _ => false,
                };

                let saved_complete = self.gc_structured_context_complete;
                if saved_complete && !direct_nested_gc_proven_shape {
                    self.gc_structured_context_complete = false;
                }
                let emitted = self.emit_ir(argument);
                self.gc_structured_context_complete = saved_complete;
                emitted?;

                let slot = self.allocate_slot();
                self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                slots.push(slot);
                if self.gc_structured_context_complete {
                    self.gc_structured_live_slots.push(slot);
                }
            }

            self.line("    movq %r12, %rdi");
            for (slot, register) in slots.iter().zip(["%rsi", "%rdx", "%rcx", "%r8", "%r9"]) {
                self.line(&format!(
                    "    movq {}(%rsp), {register}",
                    Self::slot_offset(*slot)
                ));
            }

            // First structured allocating consumer: wsm_cons. Other runtime
            // allocators stay uncertified until #412 gives them an exact
            // root-class proof.
            let gc_return_label = if runtime == "wsm_cons" && self.gc_structured_context_complete {
                let live_slots = self.gc_structured_live_slots.clone();
                Some(self.emit_gc_root_certificate(
                    "runtime-call-structured",
                    runtime,
                    &live_slots,
                    &["%rsi", "%rdx"],
                ))
            } else {
                None
            };

            self.line(&format!("    call {runtime}"));
            if let Some(return_label) = gc_return_label {
                self.line(&format!("{return_label}:"));
            }
            Ok(())
        })();

        self.gc_structured_live_slots.truncate(live_checkpoint);
        self.gc_structured_context_complete = previous_complete;
        result
    }

    fn emit_platform_call(&mut self, func: &Ir, args: &[Ir]) -> Result<(), CompileError> {
        let (_, expected, runtime) =
            platform_call_contract(func).expect("preflight classified platform call");
        debug_assert_eq!(args.len(), expected);
        self.emit_runtime_call_with_structured_args(args, runtime)
    }

    /// Emit a bounded, immediately-applied lambda with 0 to 5 parameters as a
    /// real machine call with its own lexical frame. `%rdi` remains the runtime
    /// context register. Existing lexical bindings are closure-converted by
    /// passing the parent frame pointer in `%r10` and copying bounded captures
    /// into the callee frame. First-class closure values use the same bounded
    /// fixed-arity register convention via `emit_fixed_arity_closure_value`.
    fn emit_direct_lambda_call(
        &mut self,
        params: &[String],
        body: &Ir,
        args: &[Ir],
    ) -> Result<(), CompileError> {
        let arg_slots: Vec<usize> = args
            .iter()
            .map(|argument| {
                self.emit_ir(argument)?;
                let slot = self.allocate_slot();
                self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                Ok(slot)
            })
            .collect::<Result<_, CompileError>>()?;

        self.emit_direct_lambda_call_with_slots(params, body, &arg_slots)
    }

    fn emit_pack_rest_list(
        &mut self,
        rest_slots: &[usize],
        certificate_preserved_slots: Option<&[usize]>,
    ) -> Result<usize, CompileError> {
        let mut current_cdr_slot = self.allocate_slot();
        self.emit_immediate(wsm_os_target::NIL);
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(current_cdr_slot)
        ));

        for index in (0..rest_slots.len()).rev() {
            let car_slot = rest_slots[index];
            self.line("    movq %r12, %rdi");
            self.line(&format!(
                "    movq {}(%rsp), %rsi",
                Self::slot_offset(car_slot)
            ));
            self.line(&format!(
                "    movq {}(%rsp), %rdx",
                Self::slot_offset(current_cdr_slot)
            ));

            // This bounded certificate is emitted only when the caller can
            // prove its extra preserved slots. At the allocating call, those
            // preserved values plus all not-yet-packed rest values and the
            // current cdr remain live. The current car/cdr operands are also
            // live in %rsi/%rdx and in their stack copies. After this call the
            // processed car and previous cdr locations die.
            let gc_return_label = if let Some(preserved_slots) = certificate_preserved_slots {
                let mut live_stack_slots = preserved_slots.to_vec();
                live_stack_slots.extend_from_slice(&rest_slots[..=index]);
                live_stack_slots.push(current_cdr_slot);
                Some(self.emit_gc_root_certificate(
                    "pack-rest-bounded",
                    "wsm_cons",
                    &live_stack_slots,
                    &["%rsi", "%rdx"],
                ))
            } else {
                None
            };

            self.line("    call wsm_cons");
            if let Some(return_label) = gc_return_label {
                self.line(&format!("{return_label}:"));
            }
            current_cdr_slot = self.allocate_slot();
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(current_cdr_slot)
            ));
        }
        Ok(current_cdr_slot)
    }

    /// Emit a variadic lambda application. Fixed arguments are evaluated first,
    /// then excess arguments are evaluated and packed right-to-left into a WSM list
    /// using `wsm_cons`, passing the result as the `(fixed.len() + 1)`-th argument.
    fn emit_direct_variadic_lambda_call(
        &mut self,
        fixed: &[String],
        rest: &str,
        body: &Ir,
        args: &[Ir],
    ) -> Result<(), CompileError> {
        // #415: the research certificate is complete only for the bounded
        // case where this call owns every stack slot in the current frame.
        // Any pre-existing slot or lexical binding may carry a live value
        // across the nested allocations, and #413 does not own general
        // expression liveness. Fail closed by emitting no certificate.
        let certificate_frame_is_complete = self.next_slot == 0 && self.env.is_empty();

        let fixed_count = fixed.len();
        let fixed_args = &args[..fixed_count];
        let rest_args = &args[fixed_count..];

        let mut arg_slots = Vec::with_capacity(fixed_count + 1);
        for arg in fixed_args {
            self.emit_ir(arg)?;
            let slot = self.allocate_slot();
            self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
            arg_slots.push(slot);
        }

        let mut rest_slots = Vec::with_capacity(rest_args.len());
        for arg in rest_args {
            self.emit_ir(arg)?;
            let slot = self.allocate_slot();
            self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
            rest_slots.push(slot);
        }

        let preserved_fixed_slots = arg_slots.clone();
        let certificate_preserved_slots =
            certificate_frame_is_complete.then_some(preserved_fixed_slots.as_slice());
        let current_cdr_slot =
            self.emit_pack_rest_list(&rest_slots, certificate_preserved_slots)?;
        arg_slots.push(current_cdr_slot);

        let mut effective_params = fixed.to_vec();
        effective_params.push(rest.to_string());

        self.emit_direct_lambda_call_with_slots(&effective_params, body, &arg_slots)
    }

    fn emit_direct_lambda_call_with_slots(
        &mut self,
        params: &[String],
        body: &Ir,
        arg_slots: &[usize],
    ) -> Result<(), CompileError> {
        let mut captures = self.env.clone();
        for param in params {
            captures.remove(param);
        }

        self.line("    movq %r12, %rdi");
        let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
        for (slot, register) in arg_slots.iter().zip(regs.iter()) {
            self.line(&format!(
                "    movq {}(%rsp), {register}",
                Self::slot_offset(*slot)
            ));
        }
        self.line("    movq %rsp, %r10");

        let lambda_label = self.allocate_label();
        let continuation_label = self.allocate_label();
        self.line(&format!("    call .Llambda_{lambda_label}"));
        self.line(&format!("    jmp .Llambda_after_{continuation_label}"));
        self.line(&format!(".Llambda_{lambda_label}:"));

        let mut ignored_symbols = BTreeSet::new();
        let mut body_slots = 0;
        let mut bindings: BTreeSet<String> = params.iter().cloned().collect();
        bindings.extend(captures.keys().cloned());
        preflight_lambda_body(body, &bindings, &mut ignored_symbols, &mut body_slots)?;
        let required_slots = params.len() + captures.len() + body_slots;
        let frame_slots = required_slots.max(1) | 1;
        let frame_bytes = frame_slots * 8;
        self.line(&format!("    subq ${frame_bytes}, %rsp"));

        for (i, register) in regs[..params.len()].iter().enumerate() {
            self.line(&format!(
                "    movq {register}, {}(%rsp)",
                Self::slot_offset(i)
            ));
        }

        let saved_env = core::mem::take(&mut self.env);
        let saved_next_slot = self.next_slot;
        for (i, param) in params.iter().enumerate() {
            self.env.insert(param.clone(), i);
        }
        self.next_slot = params.len();

        for (name, parent_slot) in &captures {
            let local_slot = self.next_slot;
            self.next_slot += 1;
            self.line(&format!(
                "    movq {}(%r10), %rax",
                Self::slot_offset(*parent_slot)
            ));
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(local_slot)
            ));
            self.env.insert(name.clone(), local_slot);
        }

        let old_frame_bytes = self.frame_bytes;
        self.frame_bytes = frame_bytes;
        let body_result = self.emit_ir(body);
        self.frame_bytes = old_frame_bytes;
        body_result?;
        self.env = saved_env;
        self.next_slot = saved_next_slot;

        self.line(&format!("    addq ${frame_bytes}, %rsp"));
        self.line("    ret");
        self.line(&format!(".Llambda_after_{continuation_label}:"));
        Ok(())
    }

    /// Materialize a fixed-arity (0..=5) closure in the runtime-owned closure
    /// arena. The runtime object format remains exactly definition_id +
    /// environment; fixed arity is compiler-owned metadata attached to the
    /// definition id and checked by each dynamic call site.
    ///
    /// User arguments follow the same bounded register convention as named
    /// functions (%rsi, %rdx, %rcx, %r8, %r9). The captured environment is
    /// passed separately in %r10, so it never consumes a user-argument
    /// register. The environment itself is a WSM list in the ordinary cons
    /// heap, therefore it outlives the native frame that created the closure.
    fn emit_fixed_arity_closure_value(
        &mut self,
        parameters: &[String],
        body: &Ir,
    ) -> Result<(), CompileError> {
        debug_assert!(parameters.len() <= 5);
        let captures = self.env.clone();
        let definition_id = self.allocate_label() + 1;
        self.closure_labels.insert(definition_id, parameters.len());

        // #421: certify closure allocation only when this native frame has no
        // unexplained spill locations. Captures clone the complete lexical
        // environment; if those slots exactly cover 0..next_slot and no
        // structured outer spill is pending, the closure construction owns the
        // complete current-frame root set. More complex contexts fail closed.
        let capture_slots: Vec<usize> = captures.values().copied().collect();
        let mut sorted_capture_slots = capture_slots.clone();
        sorted_capture_slots.sort_unstable();
        sorted_capture_slots.dedup();
        let expected_frame_slots: Vec<usize> = (0..self.next_slot).collect();
        let certificate_frame_is_complete = self.gc_structured_live_slots.is_empty()
            && sorted_capture_slots == expected_frame_slots;

        self.emit_immediate(wsm_os_target::NIL);
        for index in (0..capture_slots.len()).rev() {
            let slot = capture_slots[index];
            let tail_slot = self.allocate_slot();
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(tail_slot)
            ));
            self.line("    movq %r12, %rdi");
            self.line(&format!("    movq {}(%rsp), %rsi", Self::slot_offset(slot)));
            self.line(&format!(
                "    movq {}(%rsp), %rdx",
                Self::slot_offset(tail_slot)
            ));

            let gc_return_label = if certificate_frame_is_complete {
                let mut live_stack_slots = capture_slots[..=index].to_vec();
                live_stack_slots.push(tail_slot);
                Some(self.emit_gc_root_certificate(
                    "closure-capture-cons",
                    "wsm_cons",
                    &live_stack_slots,
                    &["%rsi", "%rdx"],
                ))
            } else {
                None
            };

            self.line("    call wsm_cons");
            if let Some(return_label) = gc_return_label {
                self.line(&format!("{return_label}:"));
            }
        }
        self.line("    movq %rax, %rdx");
        self.line("    movq %r12, %rdi");
        self.line(&format!("    movl ${definition_id}, %esi"));
        let gc_return_label = if certificate_frame_is_complete {
            Some(self.emit_gc_root_certificate(
                "closure-new-bounded",
                "wsm_closure_new",
                &[],
                &["%rdx"],
            ))
        } else {
            None
        };

        self.line("    call wsm_closure_new");
        if let Some(return_label) = gc_return_label {
            self.line(&format!("{return_label}:"));
        }

        let after_label = self.allocate_label();
        self.line(&format!("    jmp .Lclosure_after_{after_label}"));
        self.line(&format!(".Lclosure_{definition_id}:"));

        let mut ignored_symbols = BTreeSet::new();
        let mut body_slots = 0;
        let mut bindings: BTreeSet<String> = parameters.iter().cloned().collect();
        bindings.extend(captures.keys().cloned());
        preflight_lambda_body(body, &bindings, &mut ignored_symbols, &mut body_slots)?;

        // A native call enters with rsp == 8 (mod 16). Keep an odd number of
        // 8-byte slots so helper calls from the closure body see rsp == 0
        // (mod 16), matching the existing SysV alignment discipline.
        let environment_slot = parameters.len();
        let required_slots = parameters.len() + 1 + captures.len() + body_slots;
        let frame_slots = if required_slots % 2 == 1 {
            required_slots
        } else {
            required_slots + 1
        };
        let frame_bytes = frame_slots * 8;
        self.line(&format!("    subq ${frame_bytes}, %rsp"));

        let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
        for (index, register) in regs[..parameters.len()].iter().enumerate() {
            self.line(&format!(
                "    movq {register}, {}(%rsp)",
                Self::slot_offset(index)
            ));
        }
        self.line(&format!(
            "    movq %r10, {}(%rsp)",
            Self::slot_offset(environment_slot)
        ));

        let saved_env = core::mem::take(&mut self.env);
        let saved_next_slot = self.next_slot;
        for (index, parameter) in parameters.iter().enumerate() {
            self.env.insert(parameter.clone(), index);
        }
        self.next_slot = environment_slot + 1;

        for name in captures.keys() {
            let local_slot = self.next_slot;
            self.next_slot += 1;
            self.line("    movq %r12, %rdi");
            self.line(&format!(
                "    movq {}(%rsp), %rsi",
                Self::slot_offset(environment_slot)
            ));
            self.line("    call wsm_car");
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(local_slot)
            ));
            self.line("    movq %r12, %rdi");
            self.line(&format!(
                "    movq {}(%rsp), %rsi",
                Self::slot_offset(environment_slot)
            ));
            self.line("    call wsm_cdr");
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(environment_slot)
            ));
            self.env.insert(name.clone(), local_slot);
        }

        let old_frame_bytes = self.frame_bytes;
        self.frame_bytes = frame_bytes;
        let body_result = self.emit_ir(body);
        self.frame_bytes = old_frame_bytes;
        body_result?;
        self.env = saved_env;
        self.next_slot = saved_next_slot;
        self.line(&format!("    addq ${frame_bytes}, %rsp"));
        self.line("    ret");
        self.line(&format!(".Lclosure_after_{after_label}:"));
        Ok(())
    }

    /// Dispatch an escaping fixed-arity closure call.
    ///
    /// Closure objects still expose only definition id + environment through
    /// the ratified runtime ABI. Arity stays compiler-owned: this call site
    /// compares only against definition ids whose recorded fixed arity equals
    /// arguments.len(). A closure with a different arity therefore reaches
    /// wsm_fail(AbiViolation) rather than being invoked with a guessed ABI.
    ///
    /// Dispatch remains a linear chain over matching closure definitions.
    fn emit_named_def_call(
        &mut self,
        value_func: &Ir,
        name: &str,
        args: &[Ir],
    ) -> Result<(), CompileError> {
        // Call a named function (admitted via Def), whose registry name may be
        // an 8-bit pattern for typed SID-keyed user definitions (#238).
        // `value_func` is the expression that loads the closure object when the
        // def is not a direct machine function (data-def fallback).
        if let Some(&label) = self.functions.get(name) {
            match self.function_arities.get(name) {
                Some(DefArity::Fixed(arity)) => {
                    let arity = *arity;
                    if args.len() != arity || arity > 5 {
                        return Err(CompileError::UnsupportedVariant(
                            "App (too many args for named function)",
                        ));
                    }
                    let arg_slots: Vec<usize> = args
                        .iter()
                        .map(|arg| {
                            self.emit_ir(arg)?;
                            let slot = self.allocate_slot();
                            self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                            Ok(slot)
                        })
                        .collect::<Result<_, CompileError>>()?;
                    self.line("    movq %r12, %rdi");
                    let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
                    for (i, slot) in arg_slots.iter().enumerate() {
                        self.line(&format!(
                            "    movq {}(%rsp), {}",
                            Self::slot_offset(*slot),
                            regs[i]
                        ));
                    }
                    self.line(&format!("    call .Lfn_{label}"));
                    Ok(())
                }
                Some(DefArity::Variadic { fixed }) => {
                    let fixed = *fixed;
                    if args.len() < fixed || fixed + 1 > 5 {
                        return Err(CompileError::UnsupportedVariant(
                            "App (variadic args out of bounds)",
                        ));
                    }
                    let fixed_args = &args[..fixed];
                    let rest_args = &args[fixed..];

                    let mut arg_slots = Vec::with_capacity(fixed + 1);
                    for arg in fixed_args {
                        self.emit_ir(arg)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        arg_slots.push(slot);
                    }

                    let mut rest_slots = Vec::with_capacity(rest_args.len());
                    for arg in rest_args {
                        self.emit_ir(arg)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        rest_slots.push(slot);
                    }

                    let rest_slot = self.emit_pack_rest_list(&rest_slots, None)?;
                    arg_slots.push(rest_slot);

                    self.line("    movq %r12, %rdi");
                    let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
                    for (i, slot) in arg_slots.iter().enumerate() {
                        self.line(&format!(
                            "    movq {}(%rsp), {}",
                            Self::slot_offset(*slot),
                            regs[i]
                        ));
                    }
                    self.line(&format!("    call .Lfn_{label}"));
                    Ok(())
                }
                Some(DefArity::AllRest) => {
                    let mut rest_slots = Vec::with_capacity(args.len());
                    for arg in args {
                        self.emit_ir(arg)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        rest_slots.push(slot);
                    }

                    let rest_slot = self.emit_pack_rest_list(&rest_slots, None)?;

                    self.line("    movq %r12, %rdi");
                    self.line(&format!(
                        "    movq {}(%rsp), %rsi",
                        Self::slot_offset(rest_slot)
                    ));
                    self.line(&format!("    call .Lfn_{label}"));
                    Ok(())
                }
                _ => self.emit_fixed_arity_closure_call(value_func, args),
            }
        } else {
            self.emit_fixed_arity_closure_call(value_func, args)
        }
    }

    fn emit_fixed_arity_closure_call(
        &mut self,
        function: &Ir,
        arguments: &[Ir],
    ) -> Result<(), CompileError> {
        debug_assert!(arguments.len() <= 5);

        // my-lisp application order is operator first, then arguments
        // left-to-right. Preserve it explicitly before any runtime metadata
        // lookup for the closure object.
        self.emit_ir(function)?;
        let closure_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(closure_slot)
        ));

        let mut argument_slots = Vec::with_capacity(arguments.len());
        for argument in arguments {
            self.emit_ir(argument)?;
            let slot = self.allocate_slot();
            self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
            argument_slots.push(slot);
        }

        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movq {}(%rsp), %rsi",
            Self::slot_offset(closure_slot)
        ));
        self.line("    call wsm_closure_environment");
        let environment_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(environment_slot)
        ));

        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movq {}(%rsp), %rsi",
            Self::slot_offset(closure_slot)
        ));
        self.line("    call wsm_closure_definition");

        let known_labels: Vec<usize> = self
            .closure_labels
            .iter()
            .filter_map(|(definition_id, arity)| {
                (*arity == arguments.len()).then_some(*definition_id)
            })
            .collect();
        let end_label = self.allocate_label();
        for definition_id in known_labels {
            let next_label = self.allocate_label();
            self.line(&format!("    cmpl ${definition_id}, %eax"));
            self.line(&format!("    jne .Lclosure_dispatch_{next_label}"));

            let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
            for (slot, register) in argument_slots.iter().zip(regs.iter()) {
                self.line(&format!(
                    "    movq {}(%rsp), {register}",
                    Self::slot_offset(*slot)
                ));
            }
            self.line(&format!(
                "    movq {}(%rsp), %r10",
                Self::slot_offset(environment_slot)
            ));
            self.line(&format!("    call .Lclosure_{definition_id}"));
            self.line(&format!("    jmp .Lclosure_call_end_{end_label}"));
            self.line(&format!(".Lclosure_dispatch_{next_label}:"));
        }

        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::AbiViolation as u32
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(closure_slot)
        ));
        self.line(&format!("    movq ${}, %rcx", arguments.len()));
        self.line("    call wsm_fail");
        self.line(&format!(".Lclosure_call_end_{end_label}:"));
        Ok(())
    }

    fn emit_symbol(&mut self, name: &str) {
        // cml#13: `self.symbols` is now keyed by exact original spelling
        // (see the preflight Quoted::Sym arm) -- must not re-uppercase here,
        // that would look up the wrong (and possibly absent) key.
        let id = self.symbols[name];
        let word = wsm_os_target::encode_symbol(id).expect("preflight assigned valid symbol id");
        self.emit_immediate(word);
    }

    fn emit_cond(&mut self, branches: &[(Ir, Ir)]) -> Result<(), CompileError> {
        let end_label = self.allocate_label();
        let mut next_branch_label = self.allocate_label();

        for (test, expr) in branches {
            self.line(&format!(".Lcond_branch_{}:", next_branch_label));
            self.emit_ir(test)?;

            next_branch_label = self.allocate_label();

            self.line(&format!("    movabsq ${}, %rcx", wsm_os_target::NIL));
            self.line("    cmpq %rcx, %rax");
            self.line(&format!("    je .Lcond_branch_{}", next_branch_label));

            self.emit_ir(expr)?;
            self.line(&format!("    jmp .Lcond_end_{}", end_label));
        }

        self.line(&format!(".Lcond_branch_{}:", next_branch_label));
        self.emit_immediate(wsm_os_target::NIL);

        self.line(&format!(".Lcond_end_{}:", end_label));
        Ok(())
    }

    fn emit_cond_match(&mut self, branches: &[(Ir, Quoted, Ir)]) -> Result<(), CompileError> {
        let end_label = self.allocate_label();
        let mut next_branch_label = self.allocate_label();

        for (query, expected, body) in branches {
            self.line(&format!(".Lcm_branch_{}:", next_branch_label));

            // Emit query and save result
            self.emit_ir(query)?;
            let query_slot = self.allocate_slot();
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(query_slot)
            ));

            // Emit expected (quoted constant) and save result
            self.emit_quoted(expected)?;
            let expected_slot = self.allocate_slot();
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(expected_slot)
            ));

            // Compare query and expected using wsm_eq
            self.line("    movq %r12, %rdi");
            self.line(&format!(
                "    movq {}(%rsp), %rsi",
                Self::slot_offset(query_slot)
            ));
            self.line(&format!(
                "    movq {}(%rsp), %rdx",
                Self::slot_offset(expected_slot)
            ));
            self.line("    call wsm_eq");

            next_branch_label = self.allocate_label();

            // If equal (result is canonical T), jump to body
            self.line(&format!(
                "    movabsq ${}, %rcx",
                wsm_os_target::CANONICAL_T
            ));
            self.line("    cmpq %rcx, %rax");
            self.line(&format!("    je .Lcm_body_{}", next_branch_label));

            // Not equal, continue to next branch
            self.line(&format!("    jmp .Lcm_branch_{}", next_branch_label));

            // Body label - emit body and jump to end
            self.line(&format!(".Lcm_body_{}:", next_branch_label));
            self.emit_ir(body)?;
            self.line(&format!("    jmp .Lcm_end_{}", end_label));
        }

        // No branch matched - emit NIL
        self.line(&format!(".Lcm_branch_{}:", next_branch_label));
        self.emit_immediate(wsm_os_target::NIL);

        self.line(&format!(".Lcm_end_{}:", end_label));
        Ok(())
    }

    fn emit_quoted(&mut self, quoted: &Quoted) -> Result<(), CompileError> {
        // #422: quote-list collection is certified only when this emitter can
        // explain every older live location in the current native frame.
        // Top-level quote has no older frame roots. A quote used as one of the
        // already-proved #417 structured arguments inherits those exact roots.
        let certificate_complete =
            self.gc_structured_context_complete || (self.gc_emit_depth == 1 && self.env.is_empty());
        let outer_roots = if self.gc_structured_context_complete {
            self.gc_structured_live_slots.clone()
        } else {
            Vec::new()
        };
        let mut quote_tail_slots = Vec::new();
        self.emit_quoted_with_gc(
            quoted,
            certificate_complete,
            &outer_roots,
            &mut quote_tail_slots,
        )
    }

    fn emit_quoted_with_gc(
        &mut self,
        quoted: &Quoted,
        certificate_complete: bool,
        outer_roots: &[usize],
        quote_tail_slots: &mut Vec<usize>,
    ) -> Result<(), CompileError> {
        match quoted {
            Quoted::Int(value) => {
                let word = wsm_os_target::encode_fixnum(*value)
                    .ok_or(CompileError::FixnumOutOfRange(*value))?;
                self.emit_immediate(word);
            }
            Quoted::Sym { original, .. } => self.emit_symbol(original),
            Quoted::Str(_) => {
                return Err(CompileError::UnsupportedVariant(
                    "quoted string (target ABI has no string representation)",
                ));
            }
            Quoted::Nil => self.emit_immediate(wsm_os_target::NIL),
            Quoted::List(values) => {
                self.emit_immediate(wsm_os_target::NIL);
                for value in values.iter().rev() {
                    let tail = self.allocate_slot();
                    self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(tail)));
                    quote_tail_slots.push(tail);

                    self.emit_quoted_with_gc(
                        value,
                        certificate_complete,
                        outer_roots,
                        quote_tail_slots,
                    )?;

                    self.line("    movq %r12, %rdi");
                    self.line("    movq %rax, %rsi");
                    self.line(&format!("    movq {}(%rsp), %rdx", Self::slot_offset(tail)));

                    let gc_return_label = if certificate_complete {
                        let mut live_stack_slots = outer_roots.to_vec();
                        live_stack_slots.extend(quote_tail_slots.iter().copied());
                        Some(self.emit_gc_root_certificate(
                            "quote-bounded",
                            "wsm_cons",
                            &live_stack_slots,
                            &["%rsi", "%rdx"],
                        ))
                    } else {
                        None
                    };

                    self.line("    call wsm_cons");
                    if let Some(return_label) = gc_return_label {
                        self.line(&format!("{return_label}:"));
                    }

                    let popped = quote_tail_slots.pop();
                    debug_assert_eq!(popped, Some(tail));
                }
            }
            Quoted::DottedList(values, tail_value) => {
                self.emit_quoted_with_gc(
                    tail_value,
                    certificate_complete,
                    outer_roots,
                    quote_tail_slots,
                )?;
                for value in values.iter().rev() {
                    let tail = self.allocate_slot();
                    self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(tail)));
                    quote_tail_slots.push(tail);

                    self.emit_quoted_with_gc(
                        value,
                        certificate_complete,
                        outer_roots,
                        quote_tail_slots,
                    )?;

                    self.line("    movq %r12, %rdi");
                    self.line("    movq %rax, %rsi");
                    self.line(&format!("    movq {}(%rsp), %rdx", Self::slot_offset(tail)));

                    let gc_return_label = if certificate_complete {
                        let mut live_stack_slots = outer_roots.to_vec();
                        live_stack_slots.extend(quote_tail_slots.iter().copied());
                        Some(self.emit_gc_root_certificate(
                            "quote-bounded",
                            "wsm_cons",
                            &live_stack_slots,
                            &["%rsi", "%rdx"],
                        ))
                    } else {
                        None
                    };

                    self.line("    call wsm_cons");
                    if let Some(return_label) = gc_return_label {
                        self.line(&format!("{return_label}:"));
                    }

                    let popped = quote_tail_slots.pop();
                    debug_assert_eq!(popped, Some(tail));
                }
            }
            _ => {
                return Err(CompileError::UnsupportedVariant(
                    "unsupported Quoted node in x86 emit",
                ));
            }
        }
        Ok(())
    }

    fn emit_machine_primitive(
        &mut self,
        operation: MachineOp,
        args: &[Ir],
    ) -> Result<(), CompileError> {
        let (name, expected) = machine_primitive_contract(operation)?;
        debug_assert_eq!(args.len(), expected, "preflight checked {name} arity");

        match operation {
            MachineOp::Rdtsc => {
                self.line("    rdtsc");
                self.line("    shlq $32, %rdx");
                self.line("    orq %rdx, %rax");
                self.line("    movabsq $0x0FFFFFFFFFFFFFFF, %rcx");
                self.line("    andq %rcx, %rax");
                self.line("    shlq $3, %rax");
                self.line(&format!(
                    "    orq ${}, %rax",
                    wsm_os_target::Tag::Fixnum as u64
                ));
                Ok(())
            }
            MachineOp::PciConfigCapability
            | MachineOp::PciConfigRead16
            | MachineOp::MmioCapability
            | MachineOp::MmioRead32
            | MachineOp::MmioWrite32 => {
                // Emit as platform call to WSM runtime (same calling convention)
                let runtime = match operation {
                    MachineOp::PciConfigCapability => "wsm_pci_config_capability",
                    MachineOp::PciConfigRead16 => "wsm_pci_config_read16",
                    MachineOp::MmioCapability => "wsm_mmio_capability",
                    MachineOp::MmioRead32 => "wsm_mmio_read32",
                    MachineOp::MmioWrite32 => "wsm_mmio_write32",
                    MachineOp::Rdtsc => unreachable!("handled above"),
                };
                let slots: Vec<usize> = args
                    .iter()
                    .map(|argument| {
                        self.emit_ir(argument)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        Ok(slot)
                    })
                    .collect::<Result<_, CompileError>>()?;
                self.line("    movq %r12, %rdi");
                for (slot, register) in slots.iter().zip(["%rsi", "%rdx", "%rcx", "%r8", "%r9"]) {
                    self.line(&format!(
                        "    movq {}(%rsp), {register}",
                        Self::slot_offset(*slot)
                    ));
                }
                self.line(&format!("    call {runtime}"));
                Ok(())
            }
        }
    }

    fn emit_sid8_call(&mut self, sid: sens::Sid8, args: &[Ir]) -> Result<(), CompileError> {
        let Some((expected, runtime)) = sid8_call_contract(sid) else {
            return Err(CompileError::UnimplementedSid8(sid));
        };
        if let Some(expected) = expected {
            debug_assert_eq!(args.len(), expected, "preflight checked SID8 arity");
        }

        // Inline or backend-composed mechanisms selected directly from SID8.
        if sid == sens::sid!(00001100) {
            return self.emit_arithmetic(X86ArithmeticKind::Add, args);
        }
        if sid == sens::sid!(00001101) {
            return self.emit_arithmetic(X86ArithmeticKind::Sub, args);
        }
        if sid == sens::sid!(00001110) {
            return self.emit_arithmetic(X86ArithmeticKind::Mul, args);
        }
        if sid == sens::sid!(00011010) {
            return self.emit_exact_q_lt(args);
        }
        if sid == sens::sid!(00011101) {
            return self.emit_exact_q_le(args);
        }
        if sid == sens::sid!(00011110) {
            return self.emit_exact_q_ge(args);
        }
        if sid == sens::sid!(00010100) {
            return self.emit_quotient(args);
        }
        if sid == sens::sid!(00010011) {
            return self.emit_mod(args);
        }

        if sid == sens::sid!(00110011) {
            self.emit_ir(&args[0])?;
            self.line("    call wsm_car");
            self.line("    call wsm_car");
            return Ok(());
        }
        if sid == sens::sid!(00110100) {
            self.emit_ir(&args[0])?;
            self.line("    call wsm_cdr");
            self.line("    call wsm_car");
            return Ok(());
        }
        if sid == sens::sid!(00110101) {
            self.emit_ir(&args[0])?;
            self.line("    call wsm_cdr");
            self.line("    call wsm_cdr");
            return Ok(());
        }
        if sid == sens::sid!(00110110) {
            self.emit_ir(&args[0])?;
            self.line("    call wsm_cdr");
            self.line("    call wsm_cdr");
            self.line("    call wsm_cdr");
            self.line("    call wsm_car");
            return Ok(());
        }

        if sid == sens::sid!(00100111) {
            return self.emit_primitive_list(args);
        }

        self.emit_runtime_call_with_structured_args(args, runtime)
    }

    /// Emit a variadic List primitive: (list) -> NIL, (list a b c) -> (a b c)
    /// Builds the list right-to-left using wsm_cons.
    fn emit_primitive_list(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        if args.is_empty() {
            self.emit_immediate(wsm_os_target::NIL);
            return Ok(());
        }

        // #423: certify LIST only when every older live same-frame location
        // is already explained. A structured parent contributes its exact
        // older roots; an otherwise empty top-level frame contributes none.
        // All other contexts fail closed.
        let certificate_outer_slots = if self.gc_structured_context_complete {
            Some(self.gc_structured_live_slots.clone())
        } else if self.next_slot == 0 && self.env.is_empty() {
            Some(Vec::new())
        } else {
            None
        };

        // Evaluate left-to-right into rewriteable source slots.
        let mut slots = Vec::new();
        for arg in args {
            self.emit_ir(arg)?;
            let slot = self.allocate_slot();
            self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
            slots.push(slot);
        }

        // Build right-to-left. Pending source slots remain live; one source
        // dies per cons. The newly constructed tail lives in %rdx.
        self.emit_immediate(wsm_os_target::NIL);
        for index in (0..slots.len()).rev() {
            let slot = slots[index];
            self.line("    movq %rax, %rdx");
            self.line(&format!("    movq {}(%rsp), %rsi", Self::slot_offset(slot)));
            self.line("    movq %r12, %rdi");

            let gc_return_label = if let Some(outer_slots) = &certificate_outer_slots {
                let mut live_stack_slots = outer_slots.clone();
                live_stack_slots.extend_from_slice(&slots[..=index]);
                Some(self.emit_gc_root_certificate(
                    "list-bounded",
                    "wsm_cons",
                    &live_stack_slots,
                    &["%rsi", "%rdx"],
                ))
            } else {
                None
            };

            self.line("    call wsm_cons");
            if let Some(return_label) = gc_return_label {
                self.line(&format!("{return_label}:"));
            }
        }
        Ok(())
    }

    fn emit_exact_q_le(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        debug_assert_eq!(args.len(), 2);
        self.emit_ir(&args[0])?;
        let left_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(left_slot)
        ));
        self.emit_ir(&args[1])?;
        let right_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(right_slot)
        ));
        let type_error = self.allocate_label();
        let done = self.allocate_label();
        for slot in [left_slot, right_slot] {
            self.line(&format!("    movq {}(%rsp), %rcx", Self::slot_offset(slot)));
            self.line("    movq %rcx, %rax");
            self.line("    andq $7, %rax");
            self.line(&format!(
                "    cmpq ${}, %rax",
                wsm_os_target::Tag::Fixnum as u64
            ));
            self.line(&format!("    jne .Lexact_q_le_type_{type_error}"));
        }
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(left_slot)
        ));
        self.line("    sarq $3, %rcx");
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(right_slot)
        ));
        self.line("    sarq $3, %rdx");
        self.line("    cmpq %rdx, %rcx");
        self.line("    setle %al");
        self.line("    movzbq %al, %rax");
        self.line("    shlq $3, %rax");
        self.line(&format!(
            "    orq ${}, %rax",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line(&format!("    jmp .Lexact_q_le_done_{done}"));
        self.line(&format!(".Lexact_q_le_type_{type_error}:"));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line("    xorl %edx, %edx");
        self.line("    xorl %ecx, %ecx");
        self.line("    call wsm_fail");
        self.line(&format!(".Lexact_q_le_done_{done}:"));
        Ok(())
    }

    /// Execute semantic 1019 (exact-Q <) for the bounded fixnum domain.
    fn emit_exact_q_lt(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        debug_assert_eq!(args.len(), 2);
        self.emit_ir(&args[0])?;
        let left_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(left_slot)
        ));
        self.emit_ir(&args[1])?;
        let right_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(right_slot)
        ));
        let type_error = self.allocate_label();
        let done = self.allocate_label();
        for slot in [left_slot, right_slot] {
            self.line(&format!("    movq {}(%rsp), %rcx", Self::slot_offset(slot)));
            self.line("    movq %rcx, %rax");
            self.line("    andq $7, %rax");
            self.line(&format!(
                "    cmpq ${}, %rax",
                wsm_os_target::Tag::Fixnum as u64
            ));
            self.line(&format!("    jne .Lexact_q_lt_type_{type_error}"));
        }
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(left_slot)
        ));
        self.line("    sarq $3, %rcx");
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(right_slot)
        ));
        self.line("    sarq $3, %rdx");
        self.line("    cmpq %rdx, %rcx");
        self.line("    setl %al");
        self.line("    movzbq %al, %rax");
        self.line("    shlq $3, %rax");
        self.line(&format!(
            "    orq ${}, %rax",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line(&format!("    jmp .Lexact_q_lt_done_{done}"));
        self.line(&format!(".Lexact_q_lt_type_{type_error}:"));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line("    xorl %edx, %edx");
        self.line("    xorl %ecx, %ecx");
        self.line("    call wsm_fail");
        self.line(&format!(".Lexact_q_lt_done_{done}:"));
        Ok(())
    }

    /// Execute semantic 1018 (exact-Q >=) for the bounded fixnum domain.
    fn emit_exact_q_ge(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        debug_assert_eq!(args.len(), 2);
        self.emit_ir(&args[0])?;
        let left_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(left_slot)
        ));
        self.emit_ir(&args[1])?;
        let right_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(right_slot)
        ));
        let type_error = self.allocate_label();
        let done = self.allocate_label();
        for slot in [left_slot, right_slot] {
            self.line(&format!("    movq {}(%rsp), %rcx", Self::slot_offset(slot)));
            self.line("    movq %rcx, %rax");
            self.line("    andq $7, %rax");
            self.line(&format!(
                "    cmpq ${}, %rax",
                wsm_os_target::Tag::Fixnum as u64
            ));
            self.line(&format!("    jne .Lexact_q_ge_type_{type_error}"));
        }
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(left_slot)
        ));
        self.line("    sarq $3, %rcx");
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(right_slot)
        ));
        self.line("    sarq $3, %rdx");
        self.line("    cmpq %rdx, %rcx");
        self.line("    setge %al");
        self.line("    movzbq %al, %rax");
        self.line("    shlq $3, %rax");
        self.line(&format!(
            "    orq ${}, %rax",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line(&format!("    jmp .Lexact_q_ge_done_{done}"));
        self.line(&format!(".Lexact_q_ge_type_{type_error}:"));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line("    xorl %edx, %edx");
        self.line("    xorl %ecx, %ecx");
        self.line("    call wsm_fail");
        self.line(&format!(".Lexact_q_ge_done_{done}:"));
        Ok(())
    }

    /// Inline checked fixnum addition or subtraction.
    fn emit_arithmetic(
        &mut self,
        operation: X86ArithmeticKind,
        args: &[Ir],
    ) -> Result<(), CompileError> {
        let ok_label = self.allocate_label();

        self.emit_ir(&args[0])?;
        let slot0 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot0)
        ));

        self.emit_ir(&args[1])?;
        let slot1 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot1)
        ));

        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(slot0)
        ));
        self.line("    sarq $3, %rcx");
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(slot1)
        ));
        self.line("    sarq $3, %rdx");

        let overflow_label = self.allocate_label();
        match operation {
            X86ArithmeticKind::Add => self.line("    addq %rdx, %rcx"),
            X86ArithmeticKind::Sub => self.line("    subq %rdx, %rcx"),
            X86ArithmeticKind::Mul => self.line("    imulq %rdx, %rcx"),
        }
        self.line(&format!("    jo .Larith_overflow_{overflow_label}"));

        let min = wsm_os_target::FIXNUM_MIN;
        let max = wsm_os_target::FIXNUM_MAX;
        self.line(&format!("    movabsq ${min}, %rax"));
        self.line("    cmpq %rax, %rcx");
        self.line(&format!("    jl .Larith_overflow_{overflow_label}"));
        self.line(&format!("    movabsq ${max}, %rax"));
        self.line("    cmpq %rax, %rcx");
        self.line(&format!("    jg .Larith_overflow_{overflow_label}"));

        self.line("    shlq $3, %rcx");
        self.line(&format!(
            "    orq ${}, %rcx",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line("    movq %rcx, %rax");
        self.line(&format!("    jmp .Larith_ok_{ok_label}"));

        self.line(&format!(".Larith_overflow_{overflow_label}:"));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line("    xorl %edx, %edx");
        self.line("    xorl %ecx, %ecx");
        self.line("    call wsm_fail");

        self.line(&format!(".Larith_ok_{ok_label}:"));
        Ok(())
    }

    /// Inline signed integer quotient through hardware idivq.
    fn emit_quotient(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        let ok_label = self.allocate_label();
        let fail_label = self.allocate_label();

        self.emit_ir(&args[0])?;
        let slot0 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot0)
        ));

        self.emit_ir(&args[1])?;
        let slot1 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot1)
        ));

        self.line(&format!(
            "    movq {}(%rsp), %rax",
            Self::slot_offset(slot0)
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(slot1)
        ));

        // Untag fixnums.
        self.line("    sarq $3, %rax");
        self.line("    sarq $3, %rcx");

        // Fail closed on divisor == 0 and on MIN / -1 overflow.
        self.line("    testq %rcx, %rcx");
        self.line(&format!("    je .Lquotient_fail_{fail_label}"));
        self.line(&format!(
            "    movabsq ${}, %rdx",
            wsm_os_target::FIXNUM_MIN >> 3
        ));
        self.line("    cmpq %rdx, %rax");
        self.line(&format!("    jne .Lquotient_no_overflow_{fail_label}"));
        self.line("    cmpq $-1, %rcx");
        self.line(&format!("    je .Lquotient_fail_{fail_label}"));
        self.line(&format!(".Lquotient_no_overflow_{fail_label}:"));

        // Signed division: RAX / RCX -> RAX = quotient, RDX = remainder.
        self.line("    cqto");
        self.line("    idivq %rcx");

        // Retag quotient in RAX as fixnum.
        self.line("    shlq $3, %rax");
        self.line(&format!(
            "    orq ${}, %rax",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line(&format!("    jmp .Lquotient_ok_{ok_label}"));

        // Fail path.
        self.line(&format!(".Lquotient_fail_{fail_label}:"));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(slot0)
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(slot1)
        ));
        self.line("    call wsm_fail");

        self.line(&format!(".Lquotient_ok_{ok_label}:"));
        Ok(())
    }

    /// Inline non-negative integer mod through hardware divq.
    fn emit_mod(&mut self, args: &[Ir]) -> Result<(), CompileError> {
        let ok_label = self.allocate_label();
        let fail_label = self.allocate_label();

        self.emit_ir(&args[0])?;
        let slot0 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot0)
        ));

        self.emit_ir(&args[1])?;
        let slot1 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot1)
        ));

        self.line(&format!(
            "    movq {}(%rsp), %rax",
            Self::slot_offset(slot0)
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(slot1)
        ));

        // Untag fixnums
        self.line("    sarq $3, %rax");
        self.line("    sarq $3, %rcx");

        // Fail-closed checks per upstream contract: divisor > 0, numerator >= 0
        self.line("    testq %rcx, %rcx");
        self.line(&format!("    jle .Lmod_fail_{fail_label}"));
        self.line("    testq %rax, %rax");
        self.line(&format!("    js .Lmod_fail_{fail_label}"));

        // x86 unsigned division: RDX:RAX / RCX -> RAX = quotient, RDX = remainder
        self.line("    xorq %rdx, %rdx");
        self.line("    divq %rcx");

        // Retag remainder in RDX as fixnum
        self.line("    shlq $3, %rdx");
        self.line(&format!(
            "    orq ${}, %rdx",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line("    movq %rdx, %rax");
        self.line(&format!("    jmp .Lmod_ok_{ok_label}"));

        // Fail path
        self.line(&format!(".Lmod_fail_{fail_label}:"));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rdx",
            Self::slot_offset(slot0)
        ));
        self.line(&format!(
            "    movq {}(%rsp), %rcx",
            Self::slot_offset(slot1)
        ));
        self.line("    call wsm_fail");

        self.line(&format!(".Lmod_ok_{ok_label}:"));
        Ok(())
    }

    /// Emit IR in a tail-call context where `TailSelfCall` is lowered to a
    /// register reload and `jmp` to the loop entry label.
    fn emit_tail_body(
        &mut self,
        ir: &Ir,
        loop_label: usize,
        param_count: usize,
        variadic_fixed: Option<usize>,
    ) -> Result<(), CompileError> {
        match ir {
            Ir::TailSelfCall { args } => {
                if let Some(fixed) = variadic_fixed {
                    if args.len() < fixed {
                        return Err(CompileError::InvalidArity {
                            operation: "variadic tail self-call",
                            expected: fixed,
                            actual: args.len(),
                        });
                    }
                    let fixed_args = &args[..fixed];
                    let rest_args = &args[fixed..];

                    let mut fixed_slots = Vec::with_capacity(fixed);
                    for arg in fixed_args {
                        self.emit_ir(arg)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        fixed_slots.push(slot);
                    }

                    let mut rest_slots = Vec::with_capacity(rest_args.len());
                    for arg in rest_args {
                        self.emit_ir(arg)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        rest_slots.push(slot);
                    }

                    let rest_slot = self.emit_pack_rest_list(&rest_slots, None)?;

                    for (param_idx, &tmp) in fixed_slots.iter().enumerate() {
                        self.line(&format!("    movq {}(%rsp), %rax", Self::slot_offset(tmp)));
                        self.line(&format!(
                            "    movq %rax, {}(%rsp)",
                            Self::slot_offset(param_idx)
                        ));
                    }
                    // Place packed rest into the rest param slot (at index `fixed`)
                    self.line(&format!(
                        "    movq {}(%rsp), %rax",
                        Self::slot_offset(rest_slot)
                    ));
                    self.line(&format!(
                        "    movq %rax, {}(%rsp)",
                        Self::slot_offset(fixed)
                    ));
                } else {
                    let tmp_slots: Vec<usize> = args
                        .iter()
                        .map(|arg| {
                            self.emit_ir(arg)?;
                            let slot = self.allocate_slot();
                            self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                            Ok(slot)
                        })
                        .collect::<Result<_, CompileError>>()?;

                    for (param_idx, &tmp) in tmp_slots.iter().enumerate().take(param_count) {
                        self.line(&format!("    movq {}(%rsp), %rax", Self::slot_offset(tmp)));
                        self.line(&format!(
                            "    movq %rax, {}(%rsp)",
                            Self::slot_offset(param_idx)
                        ));
                    }
                }

                self.line(&format!("    jmp .Ltcloop_{loop_label}"));
                Ok(())
            }
            Ir::Cond { branches } => {
                self.emit_cond_tail(branches, loop_label, param_count, variadic_fixed)
            }
            Ir::Let { bindings, body } => {
                // Lisp `let` is parallel: every value form observes the same
                // enclosing lexical environment. Store all values first and
                // install the new name -> slot bindings only for the body.
                let saved_env = self.env.clone();
                let mut body_bindings = Vec::with_capacity(bindings.len());
                for (name, val) in bindings {
                    self.emit_ir(val)?;
                    let slot = self.allocate_slot();
                    self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                    body_bindings.push((name.clone(), slot));
                }
                for (name, slot) in body_bindings {
                    self.env.insert(name, slot);
                }
                let result = self.emit_tail_body(body, loop_label, param_count, variadic_fixed);
                self.env = saved_env;
                result
            }
            other => self.emit_ir(other),
        }
    }

    /// Cond emission in a tail-call context: branch bodies use `emit_tail_body`
    /// so that `TailSelfCall` nodes within them produce `jmp` rather than `call`.
    fn emit_cond_tail(
        &mut self,
        branches: &[(Ir, Ir)],
        loop_label: usize,
        param_count: usize,
        variadic_fixed: Option<usize>,
    ) -> Result<(), CompileError> {
        let end_label = self.allocate_label();
        let mut next_branch_label = self.allocate_label();

        for (test, expr) in branches {
            self.line(&format!(".Lcond_branch_{next_branch_label}:"));
            self.emit_ir(test)?;

            next_branch_label = self.allocate_label();

            self.line(&format!("    movabsq ${}, %rcx", wsm_os_target::NIL));
            self.line("    cmpq %rcx, %rax");
            self.line(&format!("    je .Lcond_branch_{next_branch_label}"));

            self.emit_tail_body(expr, loop_label, param_count, variadic_fixed)?;
            self.line(&format!("    jmp .Lcond_end_{end_label}"));
        }

        self.line(&format!(".Lcond_branch_{next_branch_label}:"));
        self.emit_immediate(wsm_os_target::NIL);

        self.line(&format!(".Lcond_end_{end_label}:"));
        Ok(())
    }
}

/// Resolve the path to the asm nucleus (`nucleus.s`), used for freestanding x86 linking and witnesses.
///
/// Discovery order:
/// 1. `WSM_NUCLEUS_ASM` environment variable (if set and points to an existing file).
/// 2. Sibling directory relative to `CARGO_MANIFEST_DIR` runtime environment variable.
/// 3. Sibling directory relative to crate manifest directory at compile-time.
/// 4. Relative to current working directory (`../wsm-my-lisp/asm/nucleus.s` or `wsm-my-lisp/asm/nucleus.s`).
///
/// Fails closed if the artifact cannot be located.
pub fn resolve_nucleus_asm_path() -> Result<std::path::PathBuf, String> {
    if let Ok(path_str) = std::env::var("WSM_NUCLEUS_ASM") {
        let path = std::path::PathBuf::from(path_str);
        if path.is_file() {
            return Ok(path);
        }
    }

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let candidate = std::path::Path::new(&manifest_dir).join("../wsm-my-lisp/asm/nucleus.s");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let compile_time_candidate =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../wsm-my-lisp/asm/nucleus.s");
    if compile_time_candidate.is_file() {
        return Ok(compile_time_candidate);
    }

    let candidate_sibling = std::path::Path::new("../wsm-my-lisp/asm/nucleus.s");
    if candidate_sibling.is_file() {
        return Ok(candidate_sibling.to_path_buf());
    }

    let candidate_local = std::path::Path::new("wsm-my-lisp/asm/nucleus.s");
    if candidate_local.is_file() {
        return Ok(candidate_local.to_path_buf());
    }

    Err(
        "x86 freestanding nucleus artifact not found: ensure wsm-my-lisp is a sibling repository or set WSM_NUCLEUS_ASM=/path/to/nucleus.s"
            .to_string(),
    )
}

#[cfg(test)]
mod symbol_capacity_tests {
    // cml#12: the boundary is real (SYMBOL_ID_MAX - 1 ordinary symbols is
    // the maximum), but SYMBOL_ID_MAX itself is 2^61 - 1 -- far too large
    // to prove by actually constructing that many distinct program symbols.
    // check_symbol_capacity is the single shared choke point both the flat
    // and tail-call paths call, so testing it directly at its exact integer
    // boundary proves the invariant without needing a real huge program.
    use super::{CompileError, check_symbol_capacity};

    #[test]
    fn max_minus_one_ordinary_symbols_is_admitted() {
        assert_eq!(
            check_symbol_capacity(wsm_os_target::SYMBOL_ID_MAX - 1),
            Ok(())
        );
    }

    #[test]
    fn exactly_symbol_id_max_ordinary_symbols_collides_with_canonical_t() {
        assert_eq!(
            check_symbol_capacity(wsm_os_target::SYMBOL_ID_MAX),
            Err(CompileError::TooManySymbols)
        );
    }

    #[test]
    fn more_than_symbol_id_max_is_also_rejected() {
        assert_eq!(
            check_symbol_capacity(wsm_os_target::SYMBOL_ID_MAX + 1),
            Err(CompileError::TooManySymbols)
        );
    }
}
