//! Deterministic GNU x86_64 assembly emitter for the `wsm-os` target ABI.
//!
//! This is deliberately a narrow first slice. It consumes admitted shared
//! [`crate::ir::Ir`] and the pinned machine-readable `wsm-os-target` crate.
//! Unsupported IR is rejected during preflight, before any assembly text is
//! produced. There is no libc, host syscall, filesystem, or C-backend fallback.

use crate::ir::{Ir, Params, PrimOp, Quoted};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    EmptyProgram,
    Unsupported(&'static str),
    InvalidArity {
        operation: &'static str,
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
            Self::Unsupported(node) => {
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
            Self::FixnumOutOfRange(value) => {
                write!(formatter, "fixnum outside wsm-os target range: {value}")
            }
            Self::TooManySymbols => write!(formatter, "symbol table exceeds wsm-os target range"),
        }
    }
}

impl std::error::Error for CompileError {}

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
                if call_name == def_name && call_args.len() == param_names.len() {
                    return self.compile_tail_call_program(def_name, param_names, body, call_args);
                }
            }
        }

        // Flat (non-tail-call) program path.
        let mut symbol_names = BTreeSet::new();
        let mut slots = 0_usize;
        for expression in program {
            preflight(expression, &mut symbol_names, &mut slots)?;
        }
        if symbol_names.len() as u64 > wsm_os_target::SYMBOL_ID_MAX {
            return Err(CompileError::TooManySymbols);
        }
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
        };
        emitter.line(".text");
        emitter.line(".globl wsm_entry");
        emitter.line(".type wsm_entry, @function");
        emitter.line("wsm_entry:");
        emitter.line("    pushq %r12");
        if frame_bytes != 0 {
            emitter.line(&format!("    subq ${frame_bytes}, %rsp"));
        }
        emitter.line("    movq %rdi, %r12");
        for expression in program {
            emitter.emit_ir(expression)?;
        }
        if frame_bytes != 0 {
            emitter.line(&format!("    addq ${frame_bytes}, %rsp"));
        }
        emitter.line("    popq %r12");
        emitter.line("    ret");
        emitter.line(".size wsm_entry, .-wsm_entry");
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
        let mut slots = 0_usize;
        preflight_tail_body(body, &mut symbol_names, &mut slots)?;
        for arg in initial_args {
            preflight(arg, &mut symbol_names, &mut slots)?;
        }
        if symbol_names.len() as u64 > wsm_os_target::SYMBOL_ID_MAX {
            return Err(CompileError::TooManySymbols);
        }
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

fn preflight(
    ir: &Ir,
    symbols: &mut BTreeSet<String>,
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
                preflight(argument, symbols, slots)?;
            }
        }
        Ir::Buffer(_) => return Err(CompileError::Unsupported("typed buffer")),
        Ir::Var(_) => {}
        Ir::Lambda {
            params: Params::Fixed(params),
            body,
        } if params.len() == 1 => {
            let bindings = BTreeSet::from([params[0].clone()]);
            preflight_lambda_body(body, &bindings, symbols, slots)?;
        }
        Ir::Lambda { .. } => return Err(CompileError::Unsupported("lambda")),
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
                    preflight(argument, symbols, slots)?;
                }
                return Ok(());
            }
            if let Ir::Lambda {
                params: Params::Fixed(params),
                body,
            } = func.as_ref()
            {
                if params.len() == 1 && args.len() == 1 {
                    preflight(&args[0], symbols, slots)?;
                    let bindings = BTreeSet::from([params[0].clone()]);
                    return preflight_lambda_body(body, &bindings, symbols, slots);
                }
            }
            if args.len() != 1 {
                return Err(CompileError::Unsupported("application"));
            }
            preflight(func, symbols, slots)?;
            preflight(&args[0], symbols, slots)?;
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight(test, symbols, slots)?;
                preflight(expr, symbols, slots)?;
            }
        }
        Ir::Let { .. } => return Err(CompileError::Unsupported("let")),
        Ir::Def { .. } => return Err(CompileError::Unsupported("def")),
        Ir::TailSelfCall { .. } => {
            return Err(CompileError::Unsupported(
                "TailSelfCall outside a tail-call program",
            ));
        }
        _ => {
            return Err(CompileError::Unsupported(
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
        Ir::Var(_) => Err(CompileError::Unsupported("unbound variable")),
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
                return Err(CompileError::Unsupported("application"));
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
        _ => Err(CompileError::Unsupported("lambda body")),
    }
}

/// Like `preflight` but permits `TailSelfCall` nodes (the body of an
/// admitted `Def`). `App`, `Lambda`, `Def` and `Var` remain rejected.
fn preflight_tail_body(
    ir: &Ir,
    symbols: &mut BTreeSet<String>,
    slots: &mut usize,
) -> Result<(), CompileError> {
    *slots += 1;
    match ir {
        Ir::TailSelfCall { args } => {
            for arg in args {
                preflight(arg, symbols, slots)?;
            }
        }
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight(test, symbols, slots)?;
                preflight_tail_body(expr, symbols, slots)?;
            }
        }
        Ir::Let { bindings, body } => {
            for (_, val) in bindings {
                preflight(val, symbols, slots)?;
            }
            preflight_tail_body(body, symbols, slots)?;
        }
        other => preflight(other, symbols, slots)?,
    }
    Ok(())
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
        // representation.  Do not silently collapse a persistent WSM FS
        // string (for example a binding name) into a symbol.
        Quoted::Str(_) => {
            return Err(CompileError::Unsupported(
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
            return Err(CompileError::Unsupported(
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
        PrimOp::EqualP => Err(CompileError::Unsupported("equal? primitive")),
    }
}

fn platform_call_contract(func: &Ir) -> Option<(&'static str, usize, &'static str)> {
    let Ir::Var(name) = func else {
        return None;
    };
    match name.as_str() {
        "PCI-CONFIG-CAPABILITY" => Some(("pci-config-capability", 0, "wsm_pci_config_capability")),
        "PCI-CONFIG-READ16" => Some(("pci-config-read16", 5, "wsm_pci_config_read16")),
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
            }
            Ir::Nil => self.emit_immediate(wsm_os_target::NIL),
            Ir::True => self.emit_immediate(wsm_os_target::TRUE),
            Ir::Quote(value) => self.emit_quoted(value)?,
            Ir::Cond { branches } => self.emit_cond(branches)?,
            Ir::Prim { op, args } => self.emit_primitive(*op, args)?,
            Ir::App { func, args } => {
                if platform_call_contract(func).is_some()
                    && !matches!(func.as_ref(), Ir::Var(name) if self.env.contains_key(name))
                {
                    self.emit_platform_call(func, args)?;
                } else if let Ir::Lambda {
                    params: Params::Fixed(params),
                    body,
                } = func.as_ref()
                {
                    if params.len() == 1 && args.len() == 1 {
                        self.emit_single_argument_lambda_call(&params[0], body, &args[0])?;
                    } else {
                        unreachable!("preflight excludes unsupported lambda application");
                    }
                } else {
                    self.emit_single_argument_closure_call(func, &args[0])?;
                }
            }
            Ir::Lambda {
                params: Params::Fixed(params),
                body,
            } if params.len() == 1 => {
                self.emit_single_argument_closure_value(&params[0], body)?;
            }
            Ir::Var(name) => {
                if let Some(&slot) = self.env.get(name) {
                    self.line(&format!("    movq {}(%rsp), %rax", Self::slot_offset(slot)));
                } else {
                    return Err(CompileError::Unsupported("unbound variable"));
                }
            }
            _ => unreachable!("preflight excludes unsupported IR"),
        }
        Ok(())
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
    /// reachable from this call site. Fine at current bounded-fixture scale
    /// (CML-CONSTITUTION-* fixtures); not yet a proven problem at larger n.
    /// Named explicitly here (CML-X86-CLOSURE-DISPATCH-SCALABILITY) so a
    /// future reader doesn't assume O(1) dispatch; see
    /// tests/x86_closure_dispatch_scale_test.rs for a benchmark that makes
    /// the linear cost measurable before any dispatch redesign (jump table,
    /// hash, sorted binary search) is considered.
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
                return Err(CompileError::Unsupported(
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
                return Err(CompileError::Unsupported(
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
    ///
    /// Both arguments are tagged fixnums: `(value << 3) | TAG_FIXNUM`.
    /// Strategy:
    ///   1. Decode both (arithmetic-right-shift by TAG_BITS=3 → signed i61).
    ///   2. Perform the 64-bit signed add/sub — `jo` fires on i64 overflow.
    ///   3. The i61 overflow boundary is tighter: check explicitly against
    ///      FIXNUM_MIN/MAX; if out of range call wsm_fail(ErrorCode::Type=2).
    ///   4. Re-encode: `shlq $3, result; orq $3, result`.
    ///
    /// Uses %rcx and %rdx as scratch; does NOT clobber %r12 (context ptr).
    fn emit_arithmetic(&mut self, operation: PrimOp, args: &[Ir]) -> Result<(), CompileError> {
        let ok_label = self.allocate_label();

        // Evaluate first arg → %rax, save to stack slot.
        self.emit_ir(&args[0])?;
        let slot0 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot0)
        ));

        // Evaluate second arg → %rax, save to stack slot.
        self.emit_ir(&args[1])?;
        let slot1 = self.allocate_slot();
        self.line(&format!(
            "    movq %rax, {}(%rsp)",
            Self::slot_offset(slot1)
        ));

        // Load and decode both operands.
        // %rcx = a (decoded i64), %rdx = b (decoded i64).
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

        // Perform the operation; check 64-bit overflow first.
        let overflow_label = self.allocate_label();
        match operation {
            PrimOp::Add => {
                self.line("    addq %rdx, %rcx");
            }
            PrimOp::Sub => {
                self.line("    subq %rdx, %rcx");
            }
            _ => unreachable!(),
        }
        // 64-bit signed overflow → Type error.
        self.line(&format!("    jo .Larith_overflow_{}", overflow_label));

        // Check 61-bit fixnum range.
        let min = wsm_os_target::FIXNUM_MIN;
        let max = wsm_os_target::FIXNUM_MAX;
        self.line(&format!("    movabsq ${min}, %rax"));
        self.line("    cmpq %rax, %rcx");
        self.line(&format!("    jl .Larith_overflow_{}", overflow_label));
        self.line(&format!("    movabsq ${max}, %rax"));
        self.line("    cmpq %rax, %rcx");
        self.line(&format!("    jg .Larith_overflow_{}", overflow_label));

        // Encode result back as fixnum.
        self.line("    shlq $3, %rcx");
        self.line(&format!(
            "    orq ${}, %rcx",
            wsm_os_target::Tag::Fixnum as u64
        ));
        self.line("    movq %rcx, %rax");
        self.line(&format!("    jmp .Larith_ok_{}", ok_label));

        // Overflow path — call wsm_fail(context, ErrorCode::Type=2, offending=0, source=0).
        self.line(&format!(".Larith_overflow_{}:", overflow_label));
        self.line("    movq %r12, %rdi");
        self.line(&format!(
            "    movl ${}, %esi",
            wsm_os_target::ErrorCode::Type as u32
        ));
        self.line("    xorl %edx, %edx");
        self.line("    xorl %ecx, %ecx");
        self.line("    call wsm_fail");

        self.line(&format!(".Larith_ok_{}:", ok_label));
        Ok(())
    }

    /// Emit IR in a tail-call context where `TailSelfCall` is lowered to a
    /// register reload and `jmp` to the loop entry label.
    ///
    /// `loop_label` is the `.Ltcloop_N` label at the top of the function body.
    /// `param_count` is the number of parameter slots (0..param_count).
    ///
    /// Any IR node other than `TailSelfCall`/`Cond`/`Let` is handed to the
    /// ordinary `emit_ir` path; the result lands in `%rax` as usual.
    fn emit_tail_body(
        &mut self,
        ir: &Ir,
        loop_label: usize,
        param_count: usize,
    ) -> Result<(), CompileError> {
        match ir {
            Ir::TailSelfCall { args } => {
                // Evaluate new arguments and store them in temporary spill
                // slots BEFORE writing to the param slots, to avoid clobbering
                // a param that is still needed as input to another arg expression.
                let tmp_slots: Vec<usize> = args
                    .iter()
                    .map(|arg| {
                        self.emit_ir(arg)?;
                        let slot = self.allocate_slot();
                        self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                        Ok(slot)
                    })
                    .collect::<Result<_, CompileError>>()?;

                // Copy tmp slots into param slots.
                for (param_idx, &tmp) in tmp_slots.iter().enumerate().take(param_count) {
                    self.line(&format!("    movq {}(%rsp), %rax", Self::slot_offset(tmp)));
                    self.line(&format!(
                        "    movq %rax, {}(%rsp)",
                        Self::slot_offset(param_idx)
                    ));
                }

                // Jump to the loop entry — no call, no new frame.
                self.line(&format!("    jmp .Ltcloop_{}", loop_label));
                Ok(())
            }
            Ir::Cond { branches } => self.emit_cond_tail(branches, loop_label, param_count),
            Ir::Let { bindings, body } => {
                // Evaluate and spill bindings (not in tail position themselves).
                for (_, val) in bindings {
                    self.emit_ir(val)?;
                    let slot = self.allocate_slot();
                    self.line(&format!("    movq %rax, {}(%rsp)", Self::slot_offset(slot)));
                }
                self.emit_tail_body(body, loop_label, param_count)
            }
            // Non-tail-call node: ordinary emit, result in %rax, epilogue follows.
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
            self.line(&format!(".Lcond_branch_{}:", next_branch_label));
            self.emit_ir(test)?;

            next_branch_label = self.allocate_label();

            self.line(&format!("    movabsq ${}, %rcx", wsm_os_target::NIL));
            self.line("    cmpq %rcx, %rax");
            self.line(&format!("    je .Lcond_branch_{}", next_branch_label));

            // Body is in tail position — use emit_tail_body.
            self.emit_tail_body(expr, loop_label, param_count)?;
            self.line(&format!("    jmp .Lcond_end_{}", end_label));
        }

        self.line(&format!(".Lcond_branch_{}:", next_branch_label));
        self.emit_immediate(wsm_os_target::NIL);

        self.line(&format!(".Lcond_end_{}:", end_label));
        Ok(())
    }
}
