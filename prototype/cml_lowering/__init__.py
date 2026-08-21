"""
CML Lowering Prototype Package
==============================
Provides compiler lowering passes for 64-bit pratyāhāra bitmasks,
compile-time set algebra constant folding, and 16-bit PVC-16 feature
comparisons into native (C/Rust) and hardware (Verilog) targets.
"""

from .pratyahara_masks import (
    CANONICAL_SOUNDS, SOUND_TO_BIT, BIT_TO_SOUND, PRATYAHARA_MASKS,
    MASK_AL, MASK_AC, MASK_HAL, get_mask, mask_intersect, mask_union,
    mask_diff, mask_complement, mask_subset, is_member, mask_to_sounds
)
from .pvc16 import (
    PhonemeVector, REGISTRY, get_phoneme, FLAG_VOWEL, STHANA_MASK,
    PRAYATNA_MASK, PRAYATNA_SPRSTA, PRAYATNA_MAHAPRANA, PRAYATNA_GHOSHA,
    PRAYATNA_ANUNASIKA, MOD_PALATALIZED
)
from .cml_ast import (
    IntLit, StrLit, SymLit, QuoteNode, ListNode, Expr, parse, to_s_expr
)
from .lowering import (
    fold_constants, lower_form, CEmitter, VerilogEmitter
)
