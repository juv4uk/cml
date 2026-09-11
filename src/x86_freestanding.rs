//! Deterministic GNU x86_64 assembly emitter for the `wsm-os` target ABI.
//!
//! This is deliberately a narrow first slice. It consumes admitted shared
//! [`crate::ir::Ir`] and the pinned machine-readable `wsm-os-target` crate.
//! Unsupported IR is rejected during preflight, before any assembly text is
//! produced. There is no libc, host syscall, filesystem, or C-backend fallback.

use crate::ir::{Ir, Params, PrimOp, Quoted};
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

#[derive(Debug, Default)]
pub struct X86FreestandingBackend;

impl X86FreestandingBackend {
    pub fn new() -> Self {
        Self
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
                if let Ir::Lambda {
                    params: Params::Fixed(params),
                    ..
                } = value.as_ref()
                {
                    def_arities.insert(name.clone(), Some(params.len()));
                } else if !matches!(value.as_ref(), Ir::Lambda { .. }) {
                    // cml#8: a data-only def is a known name (Var reads of it
                    // are fine) but not callable (None, distinct from an
                    // unknown name which is simply absent from the map).
                    def_arities.insert(name.clone(), None);
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
            closure_labels: Vec::new(),
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
        // Stage2: only top-level unary defs that are actually read as values
        // receive a closure identity. Direct calls stay direct `.Lfn_N` calls.
        // One descriptor is allocated at wsm_entry startup and re-read from a
        // generated word slot, so two reads of the same binding preserve `eq`
        // identity. This is the exact machine property meta-eval's provenance
        // tokens require; allocating a fresh descriptor per Var read would be
        // semantically wrong even if definition/environment payloads matched.
        for name in first_class_named_functions {
            let definition_id = emitter.allocate_label() + 1;
            emitter.closure_labels.push(definition_id);
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
        // A first-class named unary function reuses its ordinary named body.
        // The closure dispatcher supplies argument in %rsi and environment in
        // %rdx; top-level named closures have NIL environment and need only
        // restore the context register before entering the regular `.Lfn_N`.
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
        preflight_tail_body(body, &mut symbol_names, &mut def_arities, &mut slots)?;
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
            closure_labels: Vec::new(),
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
        emitter.emit_tail_body(body, loop_label, param_count)?;

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
        Ir::Prim { args, .. } | Ir::App { args, .. } => args.iter().any(contains_tail_self_call),
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

/// Find top-level unary definitions that are read as values rather than used
/// only in direct call position. This keeps closure allocation demand tied to
/// an actual first-class use instead of allocating descriptors for every def.
fn collect_first_class_named_functions(
    program: &[Ir],
    def_arities: &BTreeMap<String, Option<usize>>,
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
    def_arities: &BTreeMap<String, Option<usize>>,
    bound: &BTreeSet<String>,
    callee_position: bool,
    out: &mut BTreeSet<String>,
) -> Result<(), CompileError> {
    match ir {
        Ir::Var(name) if !callee_position && !bound.contains(name) => {
            // cml#8: a data-only def (None) is an ordinary value read, not a
            // first-class-function-value attempt -- only a real function
            // entry (Some(arity)) is subject to the arity-1 gate below.
            if let Some(Some(arity)) = def_arities.get(name) {
                if *arity == 1 {
                    out.insert(name.clone());
                } else {
                    return Err(CompileError::UnsupportedVariant(
                        "first-class named function (arity != 1)",
                    ));
                }
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
        Ir::Prim { args, .. } | Ir::TailSelfCall { args } => {
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
    def_arities: &mut BTreeMap<String, Option<usize>>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::Int(value) => {
            wsm_os_target::encode_fixnum(*value).ok_or(CompileError::FixnumOutOfRange(*value))?;
        }
        Ir::Nil | Ir::True => {}
        Ir::Quote(value) => preflight_quoted(value, symbols, slots)?,
        Ir::Prim { op, args } => {
            let (name, expected) = primitive_contract(*op)?;
            if args.len() != expected {
                return Err(CompileError::InvalidArity {
                    operation: name,
                    expected,
                    actual: args.len(),
                });
            }
            for argument in args {
                preflight(argument, symbols, def_arities, slots)?;
            }
            return Ok(());
        }
        Ir::Buffer(_) => return Err(CompileError::UnsupportedVariant("typed buffer")),
        Ir::Var(_) => {}
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() == 1 => {
            let bindings = BTreeSet::from([params[0].clone()]);
            preflight_lambda_body(body, &bindings, symbols, slots)?;
        }
        Ir::Lambda { .. } => return Err(CompileError::UnsupportedVariant("lambda")),
        Ir::App { func, args } => {
            if let Some((operation, expected, _)) = platform_call_contract(func) {
                if args.len() != expected {
                    return Err(CompileError::InvalidArity {
                        operation,
                        expected,
                        actual: args.len(),
                    });
                }
                for argument in args {
                    preflight(argument, symbols, def_arities, slots)?;
                }
                return Ok(());
            }
            if let Ir::Lambda {
                params: Params::Fixed(params),
                body,
            } = func.as_ref()
            {
                if params.len() == 1 && args.len() == 1 {
                    preflight(&args[0], symbols, def_arities, slots)?;
                    let bindings = BTreeSet::from([params[0].clone()]);
                    return preflight_lambda_body(body, &bindings, symbols, slots);
                }
            }
            if let Ir::Var(name) = func.as_ref() {
                match def_arities.get(name) {
                    Some(Some(arity)) => {
                        let arity = *arity;
                        if args.len() != arity {
                            return Err(CompileError::DefArityMismatch {
                                name: name.clone(),
                                expected: arity,
                                actual: args.len(),
                            });
                        }
                        for argument in args {
                            preflight(argument, symbols, def_arities, slots)?;
                        }
                        return Ok(());
                    }
                    Some(None) => {
                        // cml#8: name is a known data-only def, not callable.
                        return Err(CompileError::UnsupportedVariant(
                            "application of a data-only def",
                        ));
                    }
                    None => {}
                }
            }
            if args.len() != 1 {
                return Err(CompileError::UnsupportedVariant("application"));
            }
            preflight(func, symbols, def_arities, slots)?;
            preflight(&args[0], symbols, def_arities, slots)?;
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight(test, symbols, def_arities, slots)?;
                preflight(expr, symbols, def_arities, slots)?;
            }
        }
        Ir::Let { .. } => return Err(CompileError::UnsupportedVariant("let")),
        Ir::Def { name, value } => {
            if let Ir::Lambda {
                params: Params::Fixed(param_names),
                body,
            } = value.as_ref()
            {
                let bindings: BTreeSet<String> = param_names.iter().cloned().collect();
                def_arities.insert(name.clone(), Some(param_names.len()));
                preflight_def_body(body, &bindings, symbols, def_arities, slots)?;
                symbols.insert(name.clone());
                return Ok(());
            } else if matches!(value.as_ref(), Ir::Lambda { .. }) {
                return Err(CompileError::UnsupportedVariant(
                    "def (non-fixed-arity lambda)",
                ));
            } else {
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
        Ir::TailSelfCall { .. } => {
            return Err(CompileError::UnsupportedVariant(
                "TailSelfCall outside a tail-call program",
            ));
        }
        _ => {
            return Err(CompileError::UnsupportedVariant(
                "unsupported IR node in x86 preflight",
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
        Ir::Prim { op, args } => {
            let (name, expected) = primitive_contract(*op)?;
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
            if args.len() != 1 {
                return Err(CompileError::UnsupportedVariant("application"));
            }
            preflight_lambda_body(func, bindings, symbols, slots)?;
            preflight_lambda_body(&args[0], bindings, symbols, slots)
        }
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() == 1 => {
            let mut nested_bindings = bindings.clone();
            nested_bindings.insert(params[0].clone());
            preflight_lambda_body(body, &nested_bindings, symbols, slots)
        }
        _ => Err(CompileError::UnsupportedVariant("lambda body")),
    }
}

/// Like `preflight` but permits `TailSelfCall` nodes (the body of an
/// admitted `Def`). `App`, `Lambda`, `Def` and `Var` remain rejected.
fn preflight_tail_body(
    ir: &Ir,
    symbols: &mut BTreeSet<String>,
    def_arities: &mut BTreeMap<String, Option<usize>>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::TailSelfCall { args } => {
            for arg in args {
                preflight(arg, symbols, def_arities, slots)?;
            }
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight(test, symbols, def_arities, slots)?;
                preflight_tail_body(expr, symbols, def_arities, slots)?;
            }
        }
        Ir::Let { bindings, body } => {
            for (_, val) in bindings {
                preflight(val, symbols, def_arities, slots)?;
            }
            preflight_tail_body(body, symbols, def_arities, slots)?;
        }
        other => preflight(other, symbols, def_arities, slots)?,
    }
    Ok(())
}

/// Like `preflight` but permits `TailSelfCall` nodes and accepts the given
/// bindings as valid variables (for Def body preflight).
fn preflight_def_body(
    ir: &Ir,
    bindings: &BTreeSet<String>,
    symbols: &mut BTreeSet<String>,
    def_arities: &mut BTreeMap<String, Option<usize>>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::Var(name) if bindings.contains(name) => Ok(()),
        Ir::Var(name) => match def_arities.get(name) {
            Some(Some(1)) => Ok(()),
            Some(Some(_)) => Err(CompileError::UnsupportedVariant(
                "first-class named function (arity != 1)",
            )),
            // cml#8: a data-only def is an ordinary value read.
            Some(None) => Ok(()),
            None => Err(CompileError::UnsupportedVariant("unbound variable")),
        },
        Ir::Int(value) => {
            wsm_os_target::encode_fixnum(*value).ok_or(CompileError::FixnumOutOfRange(*value))?;
            Ok(())
        }
        Ir::Nil | Ir::True => Ok(()),
        Ir::Quote(value) => preflight_quoted(value, symbols, slots),
        Ir::Prim { op, args } => {
            let (name, expected) = primitive_contract(*op)?;
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
                        Some(Some(arity)) => {
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
                        Some(None) => {
                            return Err(CompileError::UnsupportedVariant(
                                "application of a data-only def",
                            ));
                        }
                        None => {}
                    }
                }
            }
            if args.len() != 1 {
                return Err(CompileError::UnsupportedVariant("application"));
            }
            preflight_def_body(func, bindings, symbols, def_arities, slots)?;
            preflight_def_body(&args[0], bindings, symbols, def_arities, slots)
        }
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() == 1 => {
            let mut nested_bindings = bindings.clone();
            nested_bindings.insert(params[0].clone());
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
        Quoted::Sym(name) => {
            symbols.insert(name.to_uppercase());
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

fn primitive_contract(operation: PrimOp) -> Result<(&'static str, usize), CompileError> {
    match operation {
        PrimOp::Cons => Ok(("cons", 2)),
        PrimOp::Car => Ok(("car", 1)),
        PrimOp::Cdr => Ok(("cdr", 1)),
        PrimOp::Eq => Ok(("eq", 2)),
        PrimOp::Atom => Ok(("atom", 1)),
        PrimOp::Add => Ok(("add", 2)),
        PrimOp::Sub => Ok(("sub", 2)),
        PrimOp::EqualP => Err(CompileError::UnsupportedVariant("equal? primitive")),
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

struct Emitter {
    output: String,
    symbols: BTreeMap<String, u64>,
    env: BTreeMap<String, usize>,
    next_slot: usize,
    next_label: usize,
    closure_labels: Vec<usize>,
    named_closure_definitions: BTreeMap<String, usize>,
    functions: BTreeMap<String, usize>,
    function_arities: BTreeMap<String, Option<usize>>,
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

    fn emit_ir(&mut self, ir: &Ir) -> Result<(), CompileError> {
        match ir {
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
            } if params.len() == 1 => self.emit_single_argument_closure_value(&params[0], body),
            Ir::Lambda {
                params: Params::Fixed(_),
                ..
            } => Err(CompileError::UnsupportedVariant(
                "Lambda (fixed, arity != 1)",
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
                if platform_call_contract(func).is_some()
                    && !matches!(func.as_ref(), Ir::Var(name) if self.env.contains_key(name))
                {
                    self.emit_platform_call(func, args)
                } else if let Ir::Lambda {
                    params: Params::Fixed(params),
                    body,
                } = func.as_ref()
                {
                    if params.len() == 1 && args.len() == 1 {
                        self.emit_single_argument_lambda_call(&params[0], body, &args[0])
                    } else {
                        Err(CompileError::UnsupportedVariant(
                            "App (multi-arg or non-lambda)",
                        ))
                    }
                } else if let Ir::Var(name) = func.as_ref() {
                    // Call a named function (admitted via Def)
                    if let Some(&label) = self.functions.get(name) {
                        // Evaluate arguments into registers/stack per SysV AMD64
                        if args.len() > 5 {
                            return Err(CompileError::UnsupportedVariant(
                                "App (too many args for named function)",
                            ));
                        }
                        // Evaluate all args to stack slots then load them into
                        // the target argument registers. `%rdi` stays context.
                        let arg_slots: Vec<usize> = args
                            .iter()
                            .map(|arg| {
                                self.emit_ir(arg)?;
                                let slot = self.allocate_slot();
                                self.line(&format!(
                                    "    movq %rax, {}(%rsp)",
                                    Self::slot_offset(slot)
                                ));
                                Ok(slot)
                            })
                            .collect::<Result<_, CompileError>>()?;
                        self.line("    movq %r12, %rdi");
                        let regs = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];
                        for (i, slot) in arg_slots.iter().enumerate() {
                            if i < regs.len() {
                                self.line(&format!(
                                    "    movq {}(%rsp), {}",
                                    Self::slot_offset(*slot),
                                    regs[i]
                                ));
                            }
                        }
                        self.line(&format!("    call .Lfn_{label}"));
                        Ok(())
                    } else {
                        self.emit_single_argument_closure_call(func, &args[0])
                    }
                } else {
                    self.emit_single_argument_closure_call(func, &args[0])
                }
            }
            Ir::Cond { branches } => self.emit_cond(branches),
            Ir::Let { .. } => Err(CompileError::UnsupportedVariant("Let")),
            Ir::Def { name, value } => {
                if let Ir::Lambda {
                    params: Params::Fixed(param_names),
                    body,
                } = value.as_ref()
                {
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
                            return Err(CompileError::UnsupportedVariant("Def (too many params)"));
                        }
                        param_env.insert(param.clone(), i);
                    }
                    self.line(&format!(".Ltcloop_{label}:"));
                    let old_env = std::mem::replace(&mut self.env, param_env);
                    let old_next_slot = self.next_slot;
                    self.next_slot = param_names.len();
                    self.emit_tail_body(body, label, param_names.len())?;
                    self.env = old_env;
                    self.next_slot = old_next_slot;
                    self.line(&format!("    addq ${frame_bytes}, %rsp"));
                    self.line("    ret");
                    Ok(())
                } else {
                    Err(CompileError::UnsupportedVariant(
                        "def (non-fixed-arity lambda)",
                    ))
                }
            }
            Ir::Prim { op, args } => self.emit_primitive(*op, args),
            Ir::TailSelfCall { .. } => Err(CompileError::UnsupportedVariant("TailSelfCall")),
        }
    }

    fn emit_immediate(&mut self, word: u64) {
        self.line(&format!("    movabsq ${word}, %rax"));
    }

    fn emit_platform_call(&mut self, func: &Ir, args: &[Ir]) -> Result<(), CompileError> {
        let (_, expected, runtime) =
            platform_call_contract(func).expect("preflight classified platform call");
        debug_assert_eq!(args.len(), expected);
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

    /// Emit a bounded, immediately-applied, one-argument lambda as a real
    /// machine call with its own lexical frame. `%rdi` remains the runtime
    /// context register. Existing lexical bindings are closure-converted by
    /// passing the parent frame pointer and copying bounded captures into the
    /// callee frame. First-class closure values remain rejected by preflight.
    fn emit_single_argument_lambda_call(
        &mut self,
        parameter: &str,
        body: &Ir,
        argument: &Ir,
    ) -> Result<(), CompileError> {
        let captures = self.env.clone();
        self.emit_ir(argument)?;
        self.line("    movq %rax, %rsi");
        self.line("    movq %rsp, %rdx");
        let lambda_label = self.allocate_label();
        let continuation_label = self.allocate_label();
        self.line(&format!("    call .Llambda_{lambda_label}"));
        self.line(&format!("    jmp .Llambda_after_{continuation_label}"));
        self.line(&format!(".Llambda_{lambda_label}:"));

        let mut ignored_symbols = BTreeSet::new();
        let mut body_slots = 0;
        let mut bindings = BTreeSet::from([parameter.to_string()]);
        bindings.extend(captures.keys().cloned());
        preflight_lambda_body(body, &bindings, &mut ignored_symbols, &mut body_slots)?;
        let required_slots = 1 + captures.len() + body_slots;
        let frame_slots = if required_slots % 2 == 1 {
            required_slots
        } else {
            required_slots + 1
        };
        let frame_bytes = frame_slots * 8;
        self.line(&format!("    subq ${frame_bytes}, %rsp"));
        self.line("    movq %rsi, 0(%rsp)");

        let saved_env = core::mem::take(&mut self.env);
        let saved_next_slot = self.next_slot;
        self.env.insert(parameter.to_string(), 0);
        self.next_slot = 1;
        for (name, parent_slot) in &captures {
            let local_slot = self.next_slot;
            self.next_slot += 1;
            self.line(&format!(
                "    movq {}(%rdx), %rax",
                Self::slot_offset(*parent_slot)
            ));
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(local_slot)
            ));
            self.env.insert(name.clone(), local_slot);
        }
        self.emit_ir(body)?;
        self.env = saved_env;
        self.next_slot = saved_next_slot;

        self.line(&format!("    addq ${frame_bytes}, %rsp"));
        self.line("    ret");
        self.line(&format!(".Llambda_after_{continuation_label}:"));
        Ok(())
    }

    /// Materialize a unary closure in the runtime-owned closure arena. The
    /// captured environment is a WSM list allocated in the ordinary cons heap,
    /// so it outlives the defining native frame.
    fn emit_single_argument_closure_value(
        &mut self,
        parameter: &str,
        body: &Ir,
    ) -> Result<(), CompileError> {
        let captures = self.env.clone();
        let definition_id = self.allocate_label() + 1;
        self.closure_labels.push(definition_id);

        self.emit_immediate(wsm_os_target::NIL);
        for (_, slot) in captures.iter().rev() {
            let tail_slot = self.allocate_slot();
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(tail_slot)
            ));
            self.line("    movq %r12, %rdi");
            self.line(&format!(
                "    movq {}(%rsp), %rsi",
                Self::slot_offset(*slot)
            ));
            self.line(&format!(
                "    movq {}(%rsp), %rdx",
                Self::slot_offset(tail_slot)
            ));
            self.line("    call wsm_cons");
        }
        self.line("    movq %rax, %rdx");
        self.line("    movq %r12, %rdi");
        self.line(&format!("    movl ${definition_id}, %esi"));
        self.line("    call wsm_closure_new");

        let after_label = self.allocate_label();
        self.line(&format!("    jmp .Lclosure_after_{after_label}"));
        self.line(&format!(".Lclosure_{definition_id}:"));

        let mut ignored_symbols = BTreeSet::new();
        let mut body_slots = 0;
        let mut bindings = BTreeSet::from([parameter.to_string()]);
        bindings.extend(captures.keys().cloned());
        preflight_lambda_body(body, &bindings, &mut ignored_symbols, &mut body_slots)?;
        let required_slots = 2 + captures.len() + body_slots;
        let frame_slots = if required_slots % 2 == 1 {
            required_slots
        } else {
            required_slots + 1
        };
        let frame_bytes = frame_slots * 8;
        self.line(&format!("    subq ${frame_bytes}, %rsp"));
        self.line("    movq %rsi, 0(%rsp)");
        self.line("    movq %rdx, 8(%rsp)");

        let saved_env = core::mem::take(&mut self.env);
        let saved_next_slot = self.next_slot;
        self.env.insert(parameter.to_string(), 0);
        self.next_slot = 2;
        for name in captures.keys() {
            let local_slot = self.next_slot;
            self.next_slot += 1;
            self.line("    movq %r12, %rdi");
            self.line("    movq 8(%rsp), %rsi");
            self.line("    call wsm_car");
            self.line(&format!(
                "    movq %rax, {}(%rsp)",
                Self::slot_offset(local_slot)
            ));
            self.line("    movq %r12, %rdi");
            self.line("    movq 8(%rsp), %rsi");
            self.line("    call wsm_cdr");
            self.line("    movq %rax, 8(%rsp)");
            self.env.insert(name.clone(), local_slot);
        }
        self.emit_ir(body)?;
        self.env = saved_env;
        self.next_slot = saved_next_slot;
        self.line(&format!("    addq ${frame_bytes}, %rsp"));
        self.line("    ret");
        self.line(&format!(".Lclosure_after_{after_label}:"));
        Ok(())
    }

    /// Dispatches an escaping single-argument closure call.
    ///
    /// Emits a linear `cmpl $definition_id, %eax` / `jne` chain against
    /// *every* closure definition compiled into this unit so far, falling
    /// through to `wsm_fail(AbiViolation)` on no match -- correct and
    /// honestly fail-closed, but O(n) machine instructions executed per call
    /// site, where n = total closures compiled into the unit, not just ones
    /// reachable from this call site. Fine at current bounded-fixture scale.
    fn emit_single_argument_closure_call(
        &mut self,
        function: &Ir,
        argument: &Ir,
    ) -> Result<(), CompileError> {
        self.emit_ir(function)?;
        let closure_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(closure_slot)
        ));
        self.emit_ir(argument)?;
        let argument_slot = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(argument_slot)
        ));

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

        let known_labels = self.closure_labels.clone();
        let end_label = self.allocate_label();
        for definition_id in known_labels {
            let next_label = self.allocate_label();
            self.line(&format!("    cmpl ${definition_id}, %eax"));
            self.line(&format!("    jne .Lclosure_dispatch_{next_label}"));
            self.line(&format!(
                "    movq {}(%rsp), %rsi",
                Self::slot_offset(argument_slot)
            ));
            self.line(&format!(
                "    movq {}(%rsp), %rdx",
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
        self.line("    xorl %ecx, %ecx");
        self.line("    call wsm_fail");
        self.line(&format!(".Lclosure_call_end_{end_label}:"));
        Ok(())
    }

    fn emit_symbol(&mut self, name: &str) {
        let id = self.symbols[&name.to_uppercase()];
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

    fn emit_quoted(&mut self, quoted: &Quoted) -> Result<(), CompileError> {
        match quoted {
            Quoted::Int(value) => {
                let word = wsm_os_target::encode_fixnum(*value)
                    .ok_or(CompileError::FixnumOutOfRange(*value))?;
                self.emit_immediate(word);
            }
            Quoted::Sym(name) => self.emit_symbol(name),
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
                    self.emit_quoted(value)?;
                    self.line("    movq %r12, %rdi");
                    self.line("    movq %rax, %rsi");
                    self.line(&format!("    movq {}(%rsp), %rdx", Self::slot_offset(tail)));
                    self.line("    call wsm_cons");
                }
            }
            Quoted::DottedList(values, tail_value) => {
                self.emit_quoted(tail_value)?;
                for value in values.iter().rev() {
                    let tail = self.allocate_slot();
                    self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(tail)));
                    self.emit_quoted(value)?;
                    self.line("    movq %r12, %rdi");
                    self.line("    movq %rax, %rsi");
                    self.line(&format!("    movq {}(%rsp), %rdx", Self::slot_offset(tail)));
                    self.line("    call wsm_cons");
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

    fn emit_primitive(&mut self, operation: PrimOp, args: &[Ir]) -> Result<(), CompileError> {
        let (name, expected) = primitive_contract(operation)?;
        debug_assert_eq!(args.len(), expected, "preflight checked {name} arity");

        // Arithmetic is inline — no runtime call, checked for 61-bit overflow.
        if matches!(operation, PrimOp::Add | PrimOp::Sub) {
            return self.emit_arithmetic(operation, args);
        }

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
        self.line(&format!(
            "    movq {}(%rsp), %rsi",
            Self::slot_offset(slots[0])
        ));
        if slots.len() == 2 {
            self.line(&format!(
                "    movq {}(%rsp), %rdx",
                Self::slot_offset(slots[1])
            ));
        }
        let runtime = match operation {
            PrimOp::Cons => "wsm_cons",
            PrimOp::Car => "wsm_car",
            PrimOp::Cdr => "wsm_cdr",
            PrimOp::Eq => "wsm_eq",
            PrimOp::Atom => "wsm_atom",
            _ => unreachable!("arithmetic handled above; equal? excluded by preflight"),
        };
        self.line(&format!("    call {runtime}"));
        Ok(())
    }

    /// Inline checked fixnum addition or subtraction.
    fn emit_arithmetic(&mut self, operation: PrimOp, args: &[Ir]) -> Result<(), CompileError> {
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
            PrimOp::Add => self.line("    addq %rdx, %rcx"),
            PrimOp::Sub => self.line("    subq %rdx, %rcx"),
            _ => unreachable!(),
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

    /// Emit IR in a tail-call context where `TailSelfCall` is lowered to a
    /// register reload and `jmp` to the loop entry label.
    fn emit_tail_body(
        &mut self,
        ir: &Ir,
        loop_label: usize,
        param_count: usize,
    ) -> Result<(), CompileError> {
        match ir {
            Ir::TailSelfCall { args } => {
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

                self.line(&format!("    jmp .Ltcloop_{loop_label}"));
                Ok(())
            }
            Ir::Cond { branches } => self.emit_cond_tail(branches, loop_label, param_count),
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
                let result = self.emit_tail_body(body, loop_label, param_count);
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

            self.emit_tail_body(expr, loop_label, param_count)?;
            self.line(&format!("    jmp .Lcond_end_{end_label}"));
        }

        self.line(&format!(".Lcond_branch_{next_branch_label}:"));
        self.emit_immediate(wsm_os_target::NIL);

        self.line(&format!(".Lcond_end_{end_label}:"));
        Ok(())
    }
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
