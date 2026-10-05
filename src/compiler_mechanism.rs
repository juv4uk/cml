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
}

impl CompilerMechanismRef {
    /// Deterministic provenance label suitable for compiler artifacts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SlotVmCar => "cml.slot-vm.car",
            Self::SlotVmCdr => "cml.slot-vm.cdr",
        }
    }

    /// Materialize the already-selected SLOT-VM mechanism.
    ///
    /// No SENS meaning is inferred here: the caller supplies the mechanism ref
    /// chosen from a verified upstream execution role.
    pub const fn slot_instruction(self, dst: Slot, pair: Slot) -> SlotInstr {
        match self {
            Self::SlotVmCar => SlotInstr::Car { dst, pair },
            Self::SlotVmCdr => SlotInstr::Cdr { dst, pair },
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_selector_roles_bind_to_slot_mechanisms_without_identity_decode() {
        assert_eq!(
            select_slot_vm_mechanism(CompilerExecutionRole::SelectorHead),
            CompilerMechanismRef::SlotVmCar
        );
        assert_eq!(
            select_slot_vm_mechanism(CompilerExecutionRole::SelectorTail),
            CompilerMechanismRef::SlotVmCdr
        );
    }

    #[test]
    fn mechanism_refs_are_stable_and_materialize_existing_slot_instructions() {
        let dst = Slot::new(2);
        let pair = Slot::new(1);

        let head = CompilerMechanismRef::SlotVmCar;
        assert_eq!(head.as_str(), "cml.slot-vm.car");
        assert_eq!(
            head.slot_instruction(dst, pair),
            SlotInstr::Car { dst, pair }
        );

        let tail = CompilerMechanismRef::SlotVmCdr;
        assert_eq!(tail.as_str(), "cml.slot-vm.cdr");
        assert_eq!(
            tail.slot_instruction(dst, pair),
            SlotInstr::Cdr { dst, pair }
        );
    }
}
