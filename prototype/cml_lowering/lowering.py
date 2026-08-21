"""
CML Lowering Engine & Target Code Generators
============================================
Implements:
1. Compile-Time Constant Folding Pass for Pratyāhāra Set Algebra
2. 64-Bit Pratyāhāra Bitmask Lowering (single-cycle member? instruction)
3. 16-Bit PVC-16 Feature Lowering (single-cycle savarna?, voicing sandhi)
4. Native C and Verilog RTL Code Emission
"""

from typing import Any, Dict, List, Optional, Tuple, Union
from .pratyahara_masks import (
    CANONICAL_SOUNDS, SOUND_TO_BIT, PRATYAHARA_MASKS, MASK_AL, MASK_AC, MASK_HAL,
    get_mask, mask_intersect, mask_union, mask_diff, mask_complement, mask_subset,
    is_member as py_is_member, mask_to_sounds
)
from .pvc16 import (
    STHANA_MASK, PRAYATNA_MASK, FLAG_VOWEL, PRAYATNA_SPRSTA, PRAYATNA_MAHAPRANA,
    PRAYATNA_GHOSHA, PRAYATNA_ANUNASIKA, MOD_PALATALIZED, PhonemeVector, REGISTRY,
    get_phoneme
)
from .cml_ast import IntLit, StrLit, SymLit, QuoteNode, ListNode, Expr, parse, to_s_expr

# ============================================================================
# PASS 1: CONSTANT FOLDING PASS FOR PRATYĀHĀRA SET ALGEBRA
# ============================================================================

def fold_constants(node: Any) -> Any:
    """
    Recursively evaluates and folds constant expressions involving pratyāhāra
    masks and set algebra into single 64-bit integer literals.
    """
    if isinstance(node, ListNode):
        # First recursively fold children
        folded_items = [fold_constants(item) for item in node.items]
        if not folded_items:
            return ListNode([])

        head = folded_items[0]
        if isinstance(head, SymLit):
            op = head.name.lower()
            args = folded_items[1:]

            # (quote sym) or (quote int)
            if op == "quote" and len(args) == 1:
                arg = args[0]
                if isinstance(arg, SymLit) and arg.name.lower() in PRATYAHARA_MASKS:
                    # 'ac -> 0x00000000000001FF
                    return IntLit(PRATYAHARA_MASKS[arg.name.lower()])
                return QuoteNode(arg)

            # (pratyahara-mask 'name) or (pratyahara 'name)
            if op in ("pratyahara-mask", "pratyahara") and len(args) == 1:
                mask = _resolve_mask_value(args[0])
                if mask is not None:
                    return IntLit(mask)

            # (intersection m1 m2) or (pratyahara-intersect m1 m2)
            if op in ("intersection", "pratyahara-intersect", "pratyahara-intersection", "bit-and", "and*") and len(args) == 2:
                m1 = _resolve_mask_value(args[0])
                m2 = _resolve_mask_value(args[1])
                if m1 is not None and m2 is not None:
                    return IntLit(mask_intersect(m1, m2))

            # (union m1 m2) or (pratyahara-union m1 m2)
            if op in ("union", "pratyahara-union", "bit-or", "or*") and len(args) == 2:
                m1 = _resolve_mask_value(args[0])
                m2 = _resolve_mask_value(args[1])
                if m1 is not None and m2 is not None:
                    return IntLit(mask_union(m1, m2))

            # (diff m1 m2) or (pratyahara-diff m1 m2)
            if op in ("diff", "difference", "pratyahara-diff", "pratyahara-difference") and len(args) == 2:
                m1 = _resolve_mask_value(args[0])
                m2 = _resolve_mask_value(args[1])
                if m1 is not None and m2 is not None:
                    return IntLit(mask_diff(m1, m2))

            # (complement m) or (pratyahara-complement m)
            if op in ("complement", "pratyahara-complement", "bit-not") and len(args) == 1:
                m = _resolve_mask_value(args[0])
                if m is not None:
                    return IntLit(mask_complement(m))

            # (subset? m1 m2) or (pratyahara-subset? m1 m2)
            if op in ("subset?", "pratyahara-subset?") and len(args) == 2:
                m1 = _resolve_mask_value(args[0])
                m2 = _resolve_mask_value(args[1])
                if m1 is not None and m2 is not None:
                    return SymLit("t") if mask_subset(m1, m2) else SymLit("nil")

        return ListNode(folded_items)

    if isinstance(node, QuoteNode):
        if isinstance(node.inner, SymLit) and node.inner.name.lower() in PRATYAHARA_MASKS:
            return IntLit(PRATYAHARA_MASKS[node.inner.name.lower()])
        return QuoteNode(fold_constants(node.inner))

    return node

def _resolve_mask_value(node: Any) -> Optional[int]:
    """Helper to extract an integer mask from various AST representations."""
    if isinstance(node, IntLit):
        return node.val
    if isinstance(node, QuoteNode) and isinstance(node.inner, SymLit):
        name = node.inner.name.lower()
        if name in PRATYAHARA_MASKS:
            return PRATYAHARA_MASKS[name]
    if isinstance(node, SymLit):
        name = node.name.lower()
        if name in PRATYAHARA_MASKS:
            return PRATYAHARA_MASKS[name]
    return None


# ============================================================================
# PASS 2 & 3: COMPILER LOWERING (AST -> IR FOR BITMASKS & PVC-16)
# ============================================================================

def lower_form(node: Any) -> Any:
    """
    Lowers high-level phonological primitives into concrete bitwise and arithmetic IR expressions.
    """
    folded = fold_constants(node)

    if not isinstance(folded, ListNode) or not folded.items:
        return folded

    head = folded.items[0]
    if not isinstance(head, SymLit):
        return ListNode([lower_form(x) for x in folded.items])

    op = head.name.lower()
    args = [lower_form(x) for x in folded.items[1:]]

    # 1. 64-Bit Pratyāhāra Membership: (member? char mask) / (pratyahara-member? char mask)
    if op in ("member?", "pratyahara-member?", "in-pratyahara?") and len(args) == 2:
        char_expr, mask_expr = args[0], args[1]
        # Lowering: (!= (& (>> mask char) 1) 0) or ((1ULL << char) & mask)
        return ListNode([
            SymLit("bit-test-pratyahara"),
            char_expr,
            mask_expr
        ])

    # 2. PVC-16 Savarṇa Homogeneity: (savarna? a b) -> Sūtra 1.1.9
    if op in ("savarna?", "is-savarna?") and len(args) == 2:
        a_expr, b_expr = args[0], args[1]
        return ListNode([
            SymLit("pvc16-savarna"),
            a_expr,
            b_expr
        ])

    # 3. PVC-16 Feature Predicates
    if op in ("vowel?", "is-vowel?", "ac?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit"), args[0], IntLit(FLAG_VOWEL)])

    if op in ("consonant?", "is-consonant?", "hal?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit-zero"), args[0], IntLit(FLAG_VOWEL)])

    if op in ("voiced?", "is-voiced?", "ghosha?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit"), args[0], IntLit(PRAYATNA_GHOSHA)])

    if op in ("aspirate?", "is-aspirate?", "mahaprana?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit"), args[0], IntLit(PRAYATNA_MAHAPRANA)])

    if op in ("nasal?", "is-nasal?", "anunasika?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit"), args[0], IntLit(PRAYATNA_ANUNASIKA)])

    if op in ("stop?", "is-stop?", "sprsta?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit"), args[0], IntLit(PRAYATNA_SPRSTA)])

    if op in ("palatalized?", "is-palatalized?") and len(args) == 1:
        return ListNode([SymLit("pvc16-test-bit"), args[0], IntLit(MOD_PALATALIZED)])

    # 4. PVC-16 Transformations (Sandhi)
    if op in ("with-voicing", "sandhi-voice") and len(args) == 1:
        return ListNode([SymLit("pvc16-set-bit"), args[0], IntLit(PRAYATNA_GHOSHA)])

    if op in ("with-devoicing", "sandhi-devoice") and len(args) == 1:
        return ListNode([SymLit("pvc16-clear-bit"), args[0], IntLit(PRAYATNA_GHOSHA)])

    if op in ("with-palatalization", "sandhi-palatalize") and len(args) == 1:
        return ListNode([SymLit("pvc16-set-bit"), args[0], IntLit(MOD_PALATALIZED)])

    return ListNode([SymLit(op)] + args)


# ============================================================================
# TARGET 1: NATIVE C EMITTER
# ============================================================================

class CEmitter:
    """Emits optimized C99 code with inlined bitmask tests and PVC-16 logic."""

    @staticmethod
    def emit_expr(node: Any) -> str:
        if isinstance(node, IntLit):
            if node.val > 0xFFFF:
                return f"0x{node.val:016X}ULL"
            elif node.val > 255:
                return f"0x{node.val:04X}"
            else:
                return f"0x{node.val:02X}"

        if isinstance(node, StrLit):
            return f'"{node.val}"'

        if isinstance(node, SymLit):
            s = node.name.lower()
            if s == "t": return "1"
            if s == "nil": return "0"
            return node.name

        if isinstance(node, ListNode):
            if not node.items:
                return "0"
            head = node.items[0]
            if isinstance(head, SymLit):
                op = head.name
                args = node.items[1:]

                # 64-bit Bitmask Membership:
                if op == "bit-test-pratyahara":
                    char_c = CEmitter.emit_expr(args[0])
                    mask_c = CEmitter.emit_expr(args[1])
                    return f"((({mask_c}) >> ({char_c})) & 1ULL)"

                # PVC-16 Savarṇa:
                if op == "pvc16-savarna":
                    a_c = CEmitter.emit_expr(args[0])
                    b_c = CEmitter.emit_expr(args[1])
                    return f"((({a_c} & 0x003E) == ({b_c} & 0x003E)) && (({a_c} & 0x003E) != 0) && (({a_c} & 0x0041) == ({b_c} & 0x0041)))"

                # PVC-16 Bit Tests:
                if op == "pvc16-test-bit":
                    val_c = CEmitter.emit_expr(args[0])
                    mask_c = CEmitter.emit_expr(args[1])
                    return f"(({val_c} & {mask_c}) != 0)"

                if op == "pvc16-test-bit-zero":
                    val_c = CEmitter.emit_expr(args[0])
                    mask_c = CEmitter.emit_expr(args[1])
                    return f"(({val_c} & {mask_c}) == 0)"

                # PVC-16 Bit Modifiers:
                if op == "pvc16-set-bit":
                    val_c = CEmitter.emit_expr(args[0])
                    mask_c = CEmitter.emit_expr(args[1])
                    return f"({val_c} | {mask_c})"

                if op == "pvc16-clear-bit":
                    val_c = CEmitter.emit_expr(args[0])
                    mask_c = CEmitter.emit_expr(args[1])
                    return f"({val_c} & ~{mask_c})"

                # General arithmetic and logical ops:
                if op == "+" and len(args) == 2:
                    return f"({CEmitter.emit_expr(args[0])} + {CEmitter.emit_expr(args[1])})"
                if op == "==" and len(args) == 2:
                    return f"({CEmitter.emit_expr(args[0])} == {CEmitter.emit_expr(args[1])})"
                if op == "!=" and len(args) == 2:
                    return f"({CEmitter.emit_expr(args[0])} != {CEmitter.emit_expr(args[1])})"
                if op == "and" and len(args) == 2:
                    return f"({CEmitter.emit_expr(args[0])} && {CEmitter.emit_expr(args[1])})"
                if op == "or" and len(args) == 2:
                    return f"({CEmitter.emit_expr(args[0])} || {CEmitter.emit_expr(args[1])})"

                # Generic function call
                args_c = ", ".join(CEmitter.emit_expr(a) for a in args)
                return f"{op}({args_c})"

        return str(node)

    @staticmethod
    def emit_c_header() -> str:
        lines = [
            "/* CML Phonetic Lowering Runtime Header (C99) */",
            "#ifndef CML_PHONETIC_LOWERING_H",
            "#define CML_PHONETIC_LOWERING_H",
            "",
            "#include <stdint.h>",
            "#include <stdbool.h>",
            "",
            "/* 64-Bit Pratyahara Bitmask Constants */",
        ]
        for name, mask in sorted(PRATYAHARA_MASKS.items()):
            lines.append(f"#define CML_PRATYAHARA_{name.upper():<10} 0x{mask:016X}ULL")

        lines.extend([
            "",
            "/* PVC-16 Bit Constants */",
            f"#define CML_PVC16_FLAG_VOWEL       0x{FLAG_VOWEL:04X}",
            f"#define CML_PVC16_STHANA_MASK      0x{STHANA_MASK:04X}",
            f"#define CML_PVC16_PRAYATNA_MASK    0x{PRAYATNA_MASK:04X}",
            f"#define CML_PVC16_PRAYATNA_SPRSTA  0x{PRAYATNA_SPRSTA:04X}",
            f"#define CML_PVC16_PRAYATNA_GHOSHA  0x{PRAYATNA_GHOSHA:04X}",
            f"#define CML_PVC16_MOD_PALATALIZED  0x{MOD_PALATALIZED:04X}",
            "",
            "/* Inlined Single-Cycle Macros */",
            "#define CML_MEMBER_P(code, mask) ((((mask) >> (code)) & 1ULL) != 0)",
            "#define CML_SAVARNA_P(a, b) ((((a) & 0x003E) == ((b) & 0x003E)) && (((a) & 0x003E) != 0) && (((a) & 0x0041) == ((b) & 0x0041)))",
            "#define CML_VOICE(a) ((a) | 0x0100)",
            "#define CML_DEVOICE(a) ((a) & ~0x0100)",
            "",
            "#endif /* CML_PHONETIC_LOWERING_H */",
        ])
        return "\n".join(lines)


# ============================================================================
# TARGET 2: SYNTHESIZABLE VERILOG RTL EMITTER
# ============================================================================

class VerilogEmitter:
    """Emits synthesizable Verilog modules for Pratyāhāra filters and PVC-16 ALU logic."""

    @staticmethod
    def emit_pratyahara_module() -> str:
        return """// Synthesizable 64-bit Pratyāhāra ROM / LUT Bit-Test Module
module cml_pratyahara_filter (
    input  wire [5:0]  char_code,       // 0x00..0x29 (6 bits for 42 sounds)
    input  wire [63:0] pratyahara_mask, // 64-bit pratyāhāra bitmask
    output wire        is_member        // 1 cycle combinational output
);
    // Single-cycle bit selection
    assign is_member = pratyahara_mask[char_code];
endmodule
"""

    @staticmethod
    def emit_pvc16_comparator() -> str:
        return """// Synthesizable PVC-16 Comparator and Sūtra 1.1.9 Savarṇa Unit
module cml_pvc16_comparator (
    input  wire [15:0] sound_a,
    input  wire [15:0] sound_b,
    output wire        is_vowel_a,
    output wire        is_vowel_b,
    output wire        is_voiced_a,
    output wire        is_aspirate_a,
    output wire        is_palatalized_a,
    output wire        is_savarna,
    output wire [15:0] voiced_a,
    output wire [15:0] unvoiced_a
);
    // Feature extractions
    assign is_vowel_a        = sound_a[0];
    assign is_vowel_b        = sound_b[0];
    assign is_voiced_a       = sound_a[8];
    assign is_aspirate_a     = sound_a[7];
    assign is_palatalized_a  = sound_a[14];

    // Sūtra 1.1.9: tulyāsyaprayatnaṁ savarṇam
    // Same Sthāna (bits 5:1 != 0) AND same Spṛṣṭa (bit 6) AND same Vowel (bit 0)
    wire same_sthana   = (sound_a[5:1] == sound_b[5:1]) && (sound_a[5:1] != 5'b00000);
    wire same_prayatna = (sound_a[6] == sound_b[6]) && (sound_a[0] == sound_b[0]);
    assign is_savarna  = same_sthana && same_prayatna;

    // Single-cycle Sandhi Voicing & Devoicing transformations
    assign voiced_a    = sound_a | 16'h0100;  // Set Ghosha bit (bit 8)
    assign unvoiced_a  = sound_a & ~16'h0100; // Clear Ghosha bit
endmodule
"""

    @staticmethod
    def emit_sandhi_pipeline() -> str:
        return """// Synthesizable Single-Cycle Phonetic Sandhi Pipeline Unit
module cml_sandhi_unit (
    input  wire [15:0] prior_sound,  // Sound A
    input  wire [15:0] next_sound,   // Sound B
    output wire [15:0] sandhi_out,
    output wire        sandhi_applied
);
    // If prior sound is a stop (bit 6) and next sound is voiced (bit 8), apply voicing sandhi (jhash/jhashi)
    wire prior_is_stop    = prior_sound[6];
    wire next_is_voiced   = next_sound[8];
    wire trigger_voicing  = prior_is_stop && next_is_voiced;

    assign sandhi_applied = trigger_voicing;
    assign sandhi_out     = trigger_voicing ? (prior_sound | 16'h0100) : prior_sound;
endmodule
"""
