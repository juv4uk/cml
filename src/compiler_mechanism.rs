//! CML-owned target mechanism selection for verified SENS compiler roles.
//!
//! This module intentionally accepts only `sens::CompilerExecutionRole`.
//! It never receives `CoreDomainIdentity`, packed bits, surface names, or
//! legacy Sid8/Sens8 identity. SENS owns meaning; CML owns target mechanism.

use crate::slot_vm::{Slot, SlotInstr};
use sens::CompilerExecutionRole;

/// Stable CML-private reference to an admitted target mechanism.
///
/// These are target/compiler implementation references, not SENS identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilerMechanismRef {
    SlotVmCar,
    SlotVmCdr,
    SlotVmCons,
}

impl CompilerMechanismRef {
    /// Deterministic provenance label suitable for compiler artifacts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SlotVmCar => "cml.slot-vm.car",
            Self::SlotVmCdr => "cml.slot-vm.cdr",
            Self::SlotVmCons => "cml.slot-vm.cons",
        }
    }

    /// Materialize an already-selected one-input selector mechanism.
    ///
    /// Pair construction is deliberately not accepted here; callers must use
    /// `pair_instruction` so selector arity and constructor arity cannot blur.
    pub const fn selector_instruction(self, dst: Slot, pair: Slot) -> Option<SlotInstr> {
        match self {
            Self::SlotVmCar => Some(SlotInstr::Car { dst, pair }),
            Self::SlotVmCdr => Some(SlotInstr::Cdr { dst, pair }),
            Self::SlotVmCons => None,
        }
    }

    /// Materialize an already-selected two-input pair-construction mechanism.
    pub const fn pair_instruction(
        self,
        dst: Slot,
        head: Slot,
        tail: Slot,
    ) -> Option<SlotInstr> {
        match self {
            Self::SlotVmCons => Some(SlotInstr::Cons { dst, head, tail }),
            Self::SlotVmCar | Self::SlotVmCdr => None,
        }
    }
}

/// Bind one SENS-verified execution role to CML's first SLOT-VM target.
///
/// This is deliberately role -> mechanism, never identity/bits -> mechanism.
pub const fn select_slot_vm_mechanism(
    role: CompilerExecutionRole,
) -> CompilerMechanismRef {
    match role {
        CompilerExecutionRole::SelectorHead => CompilerMechanismRef::SlotVmCar,
        CompilerExecutionRole::SelectorTail => CompilerMechanismRef::SlotVmCdr,
        CompilerExecutionRole::PairConstruct => CompilerMechanismRef::SlotVmCons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_roles_bind_to_slot_mechanisms_without_identity_decode() {
        assert_eq!(
            select_slot_vm_mechanism(CompilerExecutionRole::SelectorHead),
            CompilerMechanismRef::SlotVmCar
        );
        assert_eq!(
            select_slot_vm_mechanism(CompilerExecutionRole::SelectorTail),
            CompilerMechanismRef::SlotVmCdr
        );
        assert_eq!(
            select_slot_vm_mechanism(CompilerExecutionRole::PairConstruct),
            CompilerMechanismRef::SlotVmCons
        );
    }

    #[test]
    fn mechanism_refs_are_stable_and_arity_specific() {
        let dst = Slot::new(2);
        let pair = Slot::new(1);

        let head = CompilerMechanismRef::SlotVmCar;
        assert_eq!(head.as_str(), "cml.slot-vm.car");
        assert_eq!(
            head.selector_instruction(dst, pair),
            Some(SlotInstr::Car { dst, pair })
        );
        assert_eq!(head.pair_instruction(dst, Slot::new(0), pair), None);

        let tail = CompilerMechanismRef::SlotVmCdr;
        assert_eq!(tail.as_str(), "cml.slot-vm.cdr");
        assert_eq!(
            tail.selector_instruction(dst, pair),
            Some(SlotInstr::Cdr { dst, pair })
        );
        assert_eq!(tail.pair_instruction(dst, Slot::new(0), pair), None);

        let cons = CompilerMechanismRef::SlotVmCons;
        assert_eq!(cons.as_str(), "cml.slot-vm.cons");
        assert_eq!(cons.selector_instruction(dst, pair), None);
        assert_eq!(
            cons.pair_instruction(dst, Slot::new(0), pair),
            Some(SlotInstr::Cons {
                dst,
                head: Slot::new(0),
                tail: pair,
            })
        );
    }
}
