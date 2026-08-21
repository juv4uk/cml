"""
Unit & Integration Test Suite for CML Phonetic Lowering Passes
==============================================================
Tests:
1. 64-bit Pratyāhāra Bitmasks and Canonical Sound Bit Mappings
2. Compile-Time Constant Folding for Pratyāhāra Set Algebra
3. Single-Cycle 64-bit Bitmask Membership Lowering
4. 16-bit PVC-16 Feature Lowering and Sūtra 1.1.9 Savarṇa Logic
5. Native C and Synthesizable Verilog Emitter Outputs
"""

import unittest
from .pratyahara_masks import (
    CANONICAL_SOUNDS, SOUND_TO_BIT, BIT_TO_SOUND, PRATYAHARA_MASKS,
    MASK_AL, MASK_AC, MASK_HAL, get_mask, mask_intersect, mask_union,
    mask_diff, mask_complement, mask_subset, is_member, mask_to_sounds
)
from .pvc16 import (
    PhonemeVector, REGISTRY, get_phoneme, FLAG_VOWEL, STHANA_MASK,
    PRAYATNA_MASK, PRAYATNA_SPRSTA, PRAYATNA_MAHAPRANA, PRAYATNA_GHOSHA,
    PRAYATNA_ANUNASIKA, MOD_PALATALIZED, STHANA_KANTHYA, STHANA_TALAVYA,
    STHANA_DANTYA, STHANA_OSHTHYA
)
from .cml_ast import (
    IntLit, StrLit, SymLit, QuoteNode, ListNode, parse, to_s_expr
)
from .lowering import (
    fold_constants, lower_form, CEmitter, VerilogEmitter
)


class TestPratyaharaBitmasks(unittest.TestCase):
    """Test mathematical correctness of 64-bit pratyāhāra bitmasks."""

    def test_canonical_sound_space(self):
        self.assertEqual(len(CANONICAL_SOUNDS), 42)
        self.assertEqual(len(SOUND_TO_BIT), 42)
        self.assertEqual(len(BIT_TO_SOUND), 42)

    def test_fundamental_pratyaharas(self):
        ac_mask = PRATYAHARA_MASKS["ac"]
        hal_mask = PRATYAHARA_MASKS["hal"]
        al_mask = PRATYAHARA_MASKS["al"]

        # Disjointness of ac and hal
        self.assertEqual(ac_mask & hal_mask, 0)
        # Completeness of al = ac | hal
        self.assertEqual(ac_mask | hal_mask, al_mask)
        # ac has 9 vowels (bits 0..8)
        self.assertEqual(bin(ac_mask).count('1'), 9)
        self.assertEqual(ac_mask, 0x1FF)
        # hal has 33 consonants (bits 9..41)
        self.assertEqual(bin(hal_mask).count('1'), 33)
        # al has 42 sounds
        self.assertEqual(bin(al_mask).count('1'), 42)

    def test_specific_pratyaharas(self):
        # ik: i, u, f, x (bits 1..4) -> 4 vowels
        ik_mask = PRATYAHARA_MASKS["ik"]
        self.assertEqual(bin(ik_mask).count('1'), 4)
        self.assertTrue(is_member(SOUND_TO_BIT["i"], ik_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["u"], ik_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["f"], ik_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["x"], ik_mask))
        self.assertFalse(is_member(SOUND_TO_BIT["a"], ik_mask))

        # yaR: y, v, r, l (bits 10..13) -> 4 semivowels
        yan_mask = PRATYAHARA_MASKS["yaR"]
        self.assertEqual(bin(yan_mask).count('1'), 4)
        self.assertTrue(is_member(SOUND_TO_BIT["y"], yan_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["v"], yan_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["r"], yan_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["l"], yan_mask))

        # Sar: S, z, s (bits 39..41) -> 3 sibilants
        sar_mask = PRATYAHARA_MASKS["Sar"]
        self.assertEqual(bin(sar_mask).count('1'), 3)
        self.assertTrue(is_member(SOUND_TO_BIT["S"], sar_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["z"], sar_mask))
        self.assertTrue(is_member(SOUND_TO_BIT["s"], sar_mask))


class TestConstantFolding(unittest.TestCase):
    """Test compile-time constant folding pass for pratyāhāra set algebra."""

    def test_intersection_folding(self):
        # (intersection 'ac 'ik) -> ik (0x001E)
        ast = parse("(intersection (quote ac) (quote ik))")[0]
        folded = fold_constants(ast)
        self.assertIsInstance(folded, IntLit)
        self.assertEqual(folded.val, PRATYAHARA_MASKS["ik"])

    def test_union_folding(self):
        # (union 'ak 'hal) -> al (0x3FFFFFFFFFF)
        ast = parse("(union (quote ak) (quote hal))")[0]
        folded = fold_constants(ast)
        self.assertIsInstance(folded, IntLit)
        self.assertEqual(folded.val, PRATYAHARA_MASKS["ak"] | PRATYAHARA_MASKS["hal"])

    def test_difference_folding(self):
        # (diff 'ac 'ik) -> a, e, o, E, O
        ast = parse("(diff (quote ac) (quote ik))")[0]
        folded = fold_constants(ast)
        self.assertIsInstance(folded, IntLit)
        diff_sounds = mask_to_sounds(folded.val)
        self.assertEqual(set(diff_sounds), {"a", "e", "o", "E", "O"})

    def test_nested_set_algebra_folding(self):
        # (intersection (union (quote ak) (quote yaR)) (quote ac)) -> ak
        ast = parse("(intersection (union (quote ak) (quote yaR)) (quote ac))")[0]
        folded = fold_constants(ast)
        self.assertIsInstance(folded, IntLit)
        self.assertEqual(folded.val, PRATYAHARA_MASKS["ak"])

    def test_subset_predicate_folding(self):
        # (subset? 'ik 'ac) -> t
        ast = parse("(subset? (quote ik) (quote ac))")[0]
        folded = fold_constants(ast)
        self.assertIsInstance(folded, SymLit)
        self.assertEqual(folded.name, "t")

        # (subset? 'hal 'ac) -> nil
        ast2 = parse("(subset? (quote hal) (quote ac))")[0]
        folded2 = fold_constants(ast2)
        self.assertIsInstance(folded2, SymLit)
        self.assertEqual(folded2.name, "nil")


class TestBitmaskLowering(unittest.TestCase):
    """Test 64-bit pratyāhāra membership lowering into bitwise instructions."""

    def test_member_lowering(self):
        # (member? char (quote ac)) -> (bit-test-pratyahara char 0x1FF)
        ast = parse("(member? c (quote ac))")[0]
        lowered = lower_form(ast)
        self.assertIsInstance(lowered, ListNode)
        self.assertEqual(lowered.items[0], SymLit("bit-test-pratyahara"))
        self.assertEqual(lowered.items[1], SymLit("c"))
        self.assertEqual(lowered.items[2], IntLit(PRATYAHARA_MASKS["ac"]))

    def test_member_with_folded_intersection(self):
        # (member? c (intersection (quote ac) (quote ik))) -> (bit-test-pratyahara c 0x1E)
        ast = parse("(member? c (intersection (quote ac) (quote ik)))")[0]
        lowered = lower_form(ast)
        self.assertIsInstance(lowered, ListNode)
        self.assertEqual(lowered.items[0], SymLit("bit-test-pratyahara"))
        self.assertEqual(lowered.items[2], IntLit(PRATYAHARA_MASKS["ik"]))

    def test_c_emission_for_member(self):
        ast = parse("(member? c (quote ac))")[0]
        lowered = lower_form(ast)
        c_code = CEmitter.emit_expr(lowered)
        self.assertTrue(">> (c)) & 1ULL)" in c_code)


class TestPVC16Lowering(unittest.TestCase):
    """Test 16-bit PVC-16 feature comparisons and Savarṇa homogeneity lowering."""

    def test_savarna_homogeneity(self):
        a = get_phoneme("a")
        A = get_phoneme("A")
        i = get_phoneme("i")
        k = get_phoneme("k")
        K = get_phoneme("K")
        t = get_phoneme("t")

        # Short 'a' and Long 'A' share Kanthya sthana + vowel -> Savarna
        self.assertTrue(a.is_savarna_with(A))
        # 'a' and 'i' differ in sthana -> Not Savarna
        self.assertFalse(a.is_savarna_with(i))
        # 'k' and 'K' (kh) share Kanthya sthana + Sprsta stop -> Savarna
        self.assertTrue(k.is_savarna_with(K))
        # 'k' (velar) and 't' (dental) -> Not Savarna
        self.assertFalse(k.is_savarna_with(t))

    def test_savarna_lowering(self):
        # (savarna? a b) -> (pvc16-savarna a b)
        ast = parse("(savarna? a b)")[0]
        lowered = lower_form(ast)
        self.assertIsInstance(lowered, ListNode)
        self.assertEqual(lowered.items[0], SymLit("pvc16-savarna"))

        # C emission check
        c_code = CEmitter.emit_expr(lowered)
        expected = "(((a & 0x003E) == (b & 0x003E)) && ((a & 0x003E) != 0) && ((a & 0x0041) == (b & 0x0041)))"
        self.assertEqual(c_code, expected)

    def test_feature_predicates_lowering(self):
        # (vowel? x) -> (pvc16-test-bit x 1)
        ast_vow = parse("(vowel? x)")[0]
        low_vow = lower_form(ast_vow)
        self.assertEqual(CEmitter.emit_expr(low_vow), "((x & 0x01) != 0)")

        # (voiced? x) -> (pvc16-test-bit x 0x0100)
        ast_voi = parse("(voiced? x)")[0]
        low_voi = lower_form(ast_voi)
        self.assertEqual(CEmitter.emit_expr(low_voi), "((x & 0x0100) != 0)")

        # (with-voicing x) -> (x | 0x0100)
        ast_wv = parse("(with-voicing x)")[0]
        low_wv = lower_form(ast_wv)
        self.assertEqual(CEmitter.emit_expr(low_wv), "(x | 0x0100)")

    def test_ukrainian_palatalization(self):
        t_soft = get_phoneme("t_soft")
        self.assertIsNotNone(t_soft)
        self.assertTrue(t_soft.is_palatalized)
        self.assertTrue(t_soft.is_consonant)
        self.assertEqual(t_soft.sthana, STHANA_DANTYA)


class TestTargetEmitters(unittest.TestCase):
    """Test C header and synthesizable Verilog module emission."""

    def test_c_header_generation(self):
        header = CEmitter.emit_c_header()
        self.assertIn("#define CML_PRATYAHARA_AC", header)
        self.assertIn("#define CML_PRATYAHARA_HAL", header)
        self.assertIn("#define CML_MEMBER_P", header)
        self.assertIn("#define CML_SAVARNA_P", header)

    def test_verilog_emission(self):
        mod_prat = VerilogEmitter.emit_pratyahara_module()
        self.assertIn("module cml_pratyahara_filter", mod_prat)
        self.assertIn("assign is_member = pratyahara_mask[char_code];", mod_prat)

        mod_pvc = VerilogEmitter.emit_pvc16_comparator()
        self.assertIn("module cml_pvc16_comparator", mod_pvc)
        self.assertIn("same_sthana && same_prayatna;", mod_pvc)

        mod_sandhi = VerilogEmitter.emit_sandhi_pipeline()
        self.assertIn("module cml_sandhi_unit", mod_sandhi)


if __name__ == "__main__":
    unittest.main()
