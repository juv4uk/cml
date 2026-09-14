//! Numeric specialization and proof-driven unboxed integer lowering (#55).
//!
//! # Architecture & Purpose
//!
//! Under ADR-004 and the Native Performance Roadmap (#51), this module implements
//! proof-driven unboxed integer lowering for bounded numeric regions.
//!
//! ## Authority & Scope Boundaries
//! - `my-lisp` owns language semantics, numeric types, and oracle evaluation.
//! - Backend-neutral `Ir` (`src/ir.rs`) remains completely untouched (no target,
//!   register, or boxing variants).
//! - All unboxing, boxing, domain facts, and virtual-register chaining live exclusively
//!   in the backend-local lowered layer (`LirFunction`, `src/x86_lir.rs`).
//!
//! ## Lowering Invariant
//! ```text
//! Boxed Lisp boundary
//!    -> checked/proven unbox once (if dynamic input)
//!    -> raw virtual-register arithmetic across the region
//!    -> box once only if surrounding Lisp observation requires it
//! ```
//! When the entry contract is already a bounded raw machine contract (`RawU64`),
//! zero boxing or unboxing occurs across the entire function.

use crate::ir::{Ir, PrimOp};
use crate::machine_inst::Provenance;
use crate::x86_lir::{BlockId, LirAluOp, LirFunction, LirInst, LirLowerError, LirTerminator, VReg};
use std::collections::HashMap;
use std::fmt;

/// Fixnum bounds for 61-bit signed integers in standard my-lisp tag scheme.
pub const MIN_FIXNUM: i64 = -(1i64 << 60);
pub const MAX_FIXNUM: i64 = (1i64 << 60) - 1;

/// Admitted numeric domain for an analyzed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumericDomain {
    /// Bounded raw unsigned 64-bit integer ([min, max]).
    RawU64 { min: u64, max: u64 },
    /// Bounded signed 61-bit fixnum ([min, max]).
    Fixnum { min: i64, max: i64 },
    /// Dynamic or unknown domain (general Lisp object: symbol, cons, string, or out-of-domain).
    DynamicUnknown,
}

impl fmt::Display for NumericDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RawU64 { min, max } => write!(f, "raw-u64[{min}..={max}]"),
            Self::Fixnum { min, max } => write!(f, "fixnum[{min}..={max}]"),
            Self::DynamicUnknown => write!(f, "dynamic-unknown"),
        }
    }
}

/// Physical representation of a value at a given point in the CFG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRepresentation {
    /// Raw unboxed 64-bit machine value in a virtual register.
    UnboxedRaw,
    /// Boxed/tagged fixnum pointer/word (`(val << 3) | Tag::Fixnum`).
    BoxedFixnum,
    /// General boxed/tagged dynamic Lisp object.
    BoxedDynamic,
}

impl fmt::Display for ValueRepresentation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnboxedRaw => write!(f, "unboxed-raw"),
            Self::BoxedFixnum => write!(f, "boxed-fixnum"),
            Self::BoxedDynamic => write!(f, "boxed-dynamic"),
        }
    }
}

/// Overflow status proven by analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowProof {
    /// Proven statically that the operation cannot overflow the admitted domain bounds.
    ProvenNoOverflow,
    /// Runtime check required.
    Checked,
    /// Cannot prove absence of overflow; must fall back to dynamic or fail closed.
    Unknown,
}

impl fmt::Display for OverflowProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProvenNoOverflow => write!(f, "proven-no-overflow"),
            Self::Checked => write!(f, "checked"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Boundary calling convention for function entry and return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryConvention {
    /// SysV raw 64-bit integer (e.g. `x86-lower-add-u64` from `my-lisp#118`).
    /// Zero boxing or unboxing occurs throughout the entire function.
    RawU64,
    /// Standard dynamic Lisp tagged fixnum boundary (`(val << 3) | Tag::Fixnum`).
    BoxedFixnum,
}

/// Specialization mode for lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecializationMode {
    /// Enable proof-driven numeric specialization (eliminate intermediate boxes).
    Enabled,
    /// Disable specialization (canonical boxed arithmetic at every intermediate step).
    Disabled,
}

/// Analysis fact associated with a virtual register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFact {
    pub vreg: VReg,
    pub domain: NumericDomain,
    pub representation: ValueRepresentation,
    pub overflow: OverflowProof,
    pub provenance: Provenance,
}

/// Summary report and facts of the specialization analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecializationAnalysis {
    pub mode: SpecializationMode,
    pub boundary: BoundaryConvention,
    pub unboxed_alu_count: usize,
    pub eliminated_box_count: usize,
    pub facts: HashMap<VReg, ValueFact>,
}

impl SpecializationAnalysis {
    /// Clean, deterministic, inspectable dump of all analysis facts.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "SpecializationAnalysis (mode: {:?}, boundary: {:?}):\n",
            self.mode, self.boundary
        ));
        out.push_str(&format!(
            "  unboxed ALU ops: {}\n  eliminated boxes: {}\n",
            self.unboxed_alu_count, self.eliminated_box_count
        ));
        let mut sorted_vregs: Vec<_> = self.facts.keys().cloned().collect();
        sorted_vregs.sort_by_key(|v| v.0);
        for vreg in sorted_vregs {
            let fact = &self.facts[&vreg];
            out.push_str(&format!(
                "  {}: domain={}, rep={}, overflow={}, prov={}\n",
                fact.vreg,
                fact.domain,
                fact.representation,
                fact.overflow,
                fact.provenance.description
            ));
        }
        out
    }
}

/// Context for proof-driven lowering with numeric specialization.
struct SpecLowerContext<'a> {
    func: &'a mut LirFunction,
    current_block: BlockId,
    boundary: BoundaryConvention,
    mode: SpecializationMode,
    facts: HashMap<VReg, ValueFact>,
    unboxed_alu_count: usize,
    eliminated_box_count: usize,
}

impl<'a> SpecLowerContext<'a> {
    fn new(
        func: &'a mut LirFunction,
        entry: BlockId,
        boundary: BoundaryConvention,
        mode: SpecializationMode,
    ) -> Self {
        Self {
            func,
            current_block: entry,
            boundary,
            mode,
            facts: HashMap::new(),
            unboxed_alu_count: 0,
            eliminated_box_count: 0,
        }
    }

    fn emit(&mut self, inst: LirInst) {
        if let Some(block) = self.func.block_mut(self.current_block) {
            block.instructions.push(inst);
        }
    }

    fn record_fact(
        &mut self,
        vreg: VReg,
        domain: NumericDomain,
        representation: ValueRepresentation,
        overflow: OverflowProof,
        provenance: Provenance,
    ) {
        self.facts.insert(
            vreg,
            ValueFact {
                vreg,
                domain,
                representation,
                overflow,
                provenance,
            },
        );
    }
}

/// Lowers an expression into x86 LIR with explicit facts and specialization.
fn lower_spec_expr(
    expr: &Ir,
    ctx: &mut SpecLowerContext,
) -> Result<(VReg, NumericDomain, ValueRepresentation), LirLowerError> {
    let prov = Provenance::new(Some("0104"), "numeric_specialization");

    match expr {
        Ir::Int(val) => {
            let in_fixnum_range = *val >= MIN_FIXNUM && *val <= MAX_FIXNUM;
            let domain = if in_fixnum_range {
                NumericDomain::Fixnum {
                    min: *val,
                    max: *val,
                }
            } else {
                NumericDomain::DynamicUnknown
            };

            let v_raw = ctx.func.alloc_vreg();
            ctx.emit(LirInst::Const64 {
                dst: v_raw,
                imm: *val as u64,
                provenance: prov.clone(),
            });

            ctx.record_fact(
                v_raw,
                domain.clone(),
                ValueRepresentation::UnboxedRaw,
                OverflowProof::ProvenNoOverflow,
                prov.clone(),
            );

            match ctx.mode {
                SpecializationMode::Enabled => {
                    // Under specialization, keep constant unboxed in virtual register.
                    ctx.eliminated_box_count += 1;
                    Ok((v_raw, domain, ValueRepresentation::UnboxedRaw))
                }
                SpecializationMode::Disabled => {
                    if ctx.boundary == BoundaryConvention::RawU64 {
                        Ok((v_raw, domain, ValueRepresentation::UnboxedRaw))
                    } else {
                        // Canonical unspecialized: eagerly box every fixnum constant
                        let v_boxed = ctx.func.alloc_vreg();
                        ctx.emit(LirInst::BoxFixnum {
                            dst: v_boxed,
                            src: v_raw,
                            provenance: prov.clone(),
                        });
                        ctx.record_fact(
                            v_boxed,
                            domain.clone(),
                            ValueRepresentation::BoxedFixnum,
                            OverflowProof::ProvenNoOverflow,
                            prov,
                        );
                        Ok((v_boxed, domain, ValueRepresentation::BoxedFixnum))
                    }
                }
            }
        }
        Ir::Prim { op, args } => match op {
            PrimOp::Add | PrimOp::Sub => {
                if args.len() != 2 {
                    return Err(LirLowerError::InvalidArity {
                        expected: 2,
                        found: args.len(),
                    });
                }

                let (lhs_vreg, lhs_domain, lhs_rep) = lower_spec_expr(&args[0], ctx)?;
                let (rhs_vreg, rhs_domain, rhs_rep) = lower_spec_expr(&args[1], ctx)?;

                let lir_alu_op = match op {
                    PrimOp::Add => LirAluOp::Add,
                    PrimOp::Sub => LirAluOp::Sub,
                    _ => unreachable!(),
                };

                // Check domain bounds and prove absence of overflow
                let (res_domain, overflow_proof) = match (&lhs_domain, &rhs_domain) {
                    (
                        NumericDomain::Fixnum {
                            min: min1,
                            max: max1,
                        },
                        NumericDomain::Fixnum {
                            min: min2,
                            max: max2,
                        },
                    ) => match op {
                        PrimOp::Add => {
                            let (min_sum, o1) = min1.overflowing_add(*min2);
                            let (max_sum, o2) = max1.overflowing_add(*max2);
                            if !o1 && !o2 && min_sum >= MIN_FIXNUM && max_sum <= MAX_FIXNUM {
                                (
                                    NumericDomain::Fixnum {
                                        min: min_sum,
                                        max: max_sum,
                                    },
                                    OverflowProof::ProvenNoOverflow,
                                )
                            } else {
                                (NumericDomain::DynamicUnknown, OverflowProof::Unknown)
                            }
                        }
                        PrimOp::Sub => {
                            let (min_sub, o1) = min1.overflowing_sub(*max2);
                            let (max_sub, o2) = max1.overflowing_sub(*min2);
                            if !o1 && !o2 && min_sub >= MIN_FIXNUM && max_sub <= MAX_FIXNUM {
                                (
                                    NumericDomain::Fixnum {
                                        min: min_sub,
                                        max: max_sub,
                                    },
                                    OverflowProof::ProvenNoOverflow,
                                )
                            } else {
                                (NumericDomain::DynamicUnknown, OverflowProof::Unknown)
                            }
                        }
                        _ => unreachable!(),
                    },
                    (
                        NumericDomain::RawU64 {
                            min: min1,
                            max: max1,
                        },
                        NumericDomain::RawU64 {
                            min: min2,
                            max: max2,
                        },
                    ) => match op {
                        PrimOp::Add => {
                            let (max_sum, o) = max1.overflowing_add(*max2);
                            if !o {
                                (
                                    NumericDomain::RawU64 {
                                        min: min1 + min2,
                                        max: max_sum,
                                    },
                                    OverflowProof::ProvenNoOverflow,
                                )
                            } else {
                                (NumericDomain::DynamicUnknown, OverflowProof::Unknown)
                            }
                        }
                        PrimOp::Sub => {
                            if min1 >= max2 {
                                (
                                    NumericDomain::RawU64 {
                                        min: min1 - max2,
                                        max: max1 - min2,
                                    },
                                    OverflowProof::ProvenNoOverflow,
                                )
                            } else {
                                (NumericDomain::DynamicUnknown, OverflowProof::Unknown)
                            }
                        }
                        _ => unreachable!(),
                    },
                    _ => (NumericDomain::DynamicUnknown, OverflowProof::Unknown),
                };

                // GUARD: No unboxed path without explicit proof of domain and absence of overflow!
                let can_specialize = ctx.mode == SpecializationMode::Enabled
                    && overflow_proof == OverflowProof::ProvenNoOverflow
                    && res_domain != NumericDomain::DynamicUnknown;

                if can_specialize {
                    // Proof-driven unboxed path:
                    // Ensure operands are unboxed (unbox once if they came as boxed from dynamic boundary)
                    let raw_lhs = match lhs_rep {
                        ValueRepresentation::UnboxedRaw => lhs_vreg,
                        ValueRepresentation::BoxedFixnum => {
                            let u = ctx.func.alloc_vreg();
                            ctx.emit(LirInst::UnboxFixnum {
                                dst: u,
                                src: lhs_vreg,
                                provenance: prov.clone(),
                            });
                            ctx.record_fact(
                                u,
                                lhs_domain,
                                ValueRepresentation::UnboxedRaw,
                                OverflowProof::ProvenNoOverflow,
                                prov.clone(),
                            );
                            u
                        }
                        ValueRepresentation::BoxedDynamic => {
                            return Err(LirLowerError::Unsupported(
                                "dynamic value cannot be unboxed without guard proof".to_string(),
                            ));
                        }
                    };

                    let raw_rhs = match rhs_rep {
                        ValueRepresentation::UnboxedRaw => rhs_vreg,
                        ValueRepresentation::BoxedFixnum => {
                            let u = ctx.func.alloc_vreg();
                            ctx.emit(LirInst::UnboxFixnum {
                                dst: u,
                                src: rhs_vreg,
                                provenance: prov.clone(),
                            });
                            ctx.record_fact(
                                u,
                                rhs_domain,
                                ValueRepresentation::UnboxedRaw,
                                OverflowProof::ProvenNoOverflow,
                                prov.clone(),
                            );
                            u
                        }
                        ValueRepresentation::BoxedDynamic => {
                            return Err(LirLowerError::Unsupported(
                                "dynamic value cannot be unboxed without guard proof".to_string(),
                            ));
                        }
                    };

                    let dst_raw = ctx.func.alloc_vreg();
                    ctx.emit(LirInst::Alu {
                        op: lir_alu_op,
                        dst: dst_raw,
                        lhs: raw_lhs,
                        rhs: raw_rhs,
                        provenance: prov.clone(),
                    });

                    ctx.unboxed_alu_count += 1;
                    ctx.eliminated_box_count += 1; // Saved intermediate re-boxing

                    ctx.record_fact(
                        dst_raw,
                        res_domain.clone(),
                        ValueRepresentation::UnboxedRaw,
                        overflow_proof,
                        prov,
                    );

                    Ok((dst_raw, res_domain, ValueRepresentation::UnboxedRaw))
                } else {
                    // Canonical unspecialized path (or out-of-domain/unproven overflow fallback)
                    // Every operation unboxes operands, does ALU, and re-boxes the result.
                    let raw_lhs = match lhs_rep {
                        ValueRepresentation::UnboxedRaw => lhs_vreg,
                        ValueRepresentation::BoxedFixnum => {
                            let u = ctx.func.alloc_vreg();
                            ctx.emit(LirInst::UnboxFixnum {
                                dst: u,
                                src: lhs_vreg,
                                provenance: prov.clone(),
                            });
                            u
                        }
                        ValueRepresentation::BoxedDynamic => {
                            return Err(LirLowerError::Unsupported(
                                "unproven dynamic value rejected from arithmetic lowering"
                                    .to_string(),
                            ));
                        }
                    };

                    let raw_rhs = match rhs_rep {
                        ValueRepresentation::UnboxedRaw => rhs_vreg,
                        ValueRepresentation::BoxedFixnum => {
                            let u = ctx.func.alloc_vreg();
                            ctx.emit(LirInst::UnboxFixnum {
                                dst: u,
                                src: rhs_vreg,
                                provenance: prov.clone(),
                            });
                            u
                        }
                        ValueRepresentation::BoxedDynamic => {
                            return Err(LirLowerError::Unsupported(
                                "unproven dynamic value rejected from arithmetic lowering"
                                    .to_string(),
                            ));
                        }
                    };

                    let dst_raw = ctx.func.alloc_vreg();
                    ctx.emit(LirInst::Alu {
                        op: lir_alu_op,
                        dst: dst_raw,
                        lhs: raw_lhs,
                        rhs: raw_rhs,
                        provenance: prov.clone(),
                    });

                    ctx.record_fact(
                        dst_raw,
                        res_domain.clone(),
                        ValueRepresentation::UnboxedRaw,
                        overflow_proof,
                        prov.clone(),
                    );

                    if ctx.boundary == BoundaryConvention::RawU64 {
                        Ok((dst_raw, res_domain, ValueRepresentation::UnboxedRaw))
                    } else {
                        let dst_boxed = ctx.func.alloc_vreg();
                        ctx.emit(LirInst::BoxFixnum {
                            dst: dst_boxed,
                            src: dst_raw,
                            provenance: prov.clone(),
                        });
                        ctx.record_fact(
                            dst_boxed,
                            res_domain.clone(),
                            ValueRepresentation::BoxedFixnum,
                            overflow_proof,
                            prov,
                        );
                        Ok((dst_boxed, res_domain, ValueRepresentation::BoxedFixnum))
                    }
                }
            }
            _ => Err(LirLowerError::Unsupported(format!(
                "primitive op {:?} not admitted in numeric specialization slice",
                op
            ))),
        },
        other => Err(LirLowerError::Unsupported(format!(
            "IR form {:?} is not admitted in numeric specialization slice",
            other
        ))),
    }
}

/// Lowers a semantic `Ir` program to an x86 `LirFunction` under numeric specialization.
pub fn lower_ir_to_specialized_lir(
    ir: &Ir,
    boundary: BoundaryConvention,
    mode: SpecializationMode,
) -> Result<(LirFunction, SpecializationAnalysis), LirLowerError> {
    let prov = Provenance::new(Some("0104"), "lower_ir_to_specialized_lir");
    let mut func = LirFunction::new("main", prov.clone());
    let entry = func.entry;
    let mut ctx = SpecLowerContext::new(&mut func, entry, boundary, mode);

    let (res_vreg, _domain, res_rep) = lower_spec_expr(ir, &mut ctx)?;

    // Handle return boundary: box once if required by calling convention
    let return_vreg = match (boundary, res_rep) {
        (BoundaryConvention::RawU64, ValueRepresentation::UnboxedRaw) => res_vreg,
        (BoundaryConvention::RawU64, ValueRepresentation::BoxedFixnum) => {
            let u = ctx.func.alloc_vreg();
            ctx.emit(LirInst::UnboxFixnum {
                dst: u,
                src: res_vreg,
                provenance: prov.clone(),
            });
            u
        }
        (BoundaryConvention::BoxedFixnum, ValueRepresentation::BoxedFixnum) => res_vreg,
        (BoundaryConvention::BoxedFixnum, ValueRepresentation::UnboxedRaw) => {
            // Box once at the exit boundary
            let b = ctx.func.alloc_vreg();
            ctx.emit(LirInst::BoxFixnum {
                dst: b,
                src: res_vreg,
                provenance: prov.clone(),
            });
            b
        }
        (_, ValueRepresentation::BoxedDynamic) => {
            return Err(LirLowerError::Unsupported(
                "cannot return unproven dynamic value across numeric boundary".to_string(),
            ));
        }
    };

    if let Some(block) = ctx.func.block_mut(ctx.current_block) {
        block.terminator = LirTerminator::Ret {
            val: Some(return_vreg),
            provenance: prov,
        };
    }

    let analysis = SpecializationAnalysis {
        mode,
        boundary,
        unboxed_alu_count: ctx.unboxed_alu_count,
        eliminated_box_count: ctx.eliminated_box_count,
        facts: ctx.facts,
    };

    Ok((func, analysis))
}

/// Inspects and counts physical boxing and unboxing instructions in an `LirFunction`.
pub fn count_boxing_insts(func: &LirFunction) -> (usize, usize) {
    let mut box_count = 0;
    let mut unbox_count = 0;
    for block in &func.blocks {
        for inst in &block.instructions {
            match inst {
                LirInst::BoxFixnum { .. } => box_count += 1,
                LirInst::UnboxFixnum { .. } => unbox_count += 1,
                _ => {}
            }
        }
    }
    (box_count, unbox_count)
}
