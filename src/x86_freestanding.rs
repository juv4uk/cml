//! Deterministic GNU x86_64 assembly emitter for the `wsm-os` target ABI.
//!
//! This is deliberately a narrow first slice. It consumes admitted shared
//! [`crate::ir::Ir`] and the pinned machine-readable `wsm-os-target` crate.
//! Unsupported IR is rejected during preflight, before any assembly text is
//! produced. There is no libc, host syscall, filesystem, or C-backend fallback.

use crate::ir::{Ir, PrimOp, Quoted};
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
            next_slot: 0,
            next_label: 0,
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
        Ir::Var(_) => return Err(CompileError::Unsupported("variable")),
        Ir::Lambda { .. } => return Err(CompileError::Unsupported("lambda")),
        Ir::App { .. } => return Err(CompileError::Unsupported("application")),
        Ir::Cond { branches } => {
            for (test, expr) in branches {
                preflight(test, symbols, slots)?;
                preflight(expr, symbols, slots)?;
            }
        }
        Ir::Let { .. } => return Err(CompileError::Unsupported("let")),
        Ir::Def { .. } => return Err(CompileError::Unsupported("def")),
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
        Quoted::Sym(name) | Quoted::Str(name) => {
            symbols.insert(name.to_uppercase());
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
        PrimOp::Add => Err(CompileError::Unsupported("add primitive")),
        PrimOp::EqualP => Err(CompileError::Unsupported("equal? primitive")),
    }
}

struct Emitter {
    output: String,
    symbols: BTreeMap<String, u64>,
    next_slot: usize,
    next_label: usize,
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
            _ => unreachable!("preflight excludes unsupported IR"),
        }
        Ok(())
    }

    fn emit_immediate(&mut self, word: u64) {
        self.line(&format!("    movabsq ${word}, %rax"));
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
            Quoted::Sym(name) | Quoted::Str(name) => self.emit_symbol(name),
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
        }
        Ok(())
    }

    fn emit_primitive(&mut self, operation: PrimOp, args: &[Ir]) -> Result<(), CompileError> {
        let (name, expected) = primitive_contract(operation)?;
        debug_assert_eq!(args.len(), expected, "preflight checked {name} arity");
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
            _ => unreachable!("primitive_contract excludes unsupported operations"),
        };
        self.line(&format!("    call {runtime}"));
        Ok(())
    }
}
