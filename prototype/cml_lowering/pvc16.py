"""
PVC-16: 16-Bit Phonetic Vector Code Module for CML Lowering
============================================================
Bitfield Specification:
- Bit 0       : Vowel flag (1 = ac / vowel, 0 = hal / consonant)
- Bits [5:1]  : Sthāna (Place of articulation):
                1 = Kaṇṭhya (Velar/Guttural)
                2 = Tālavya (Palatal)
                3 = Mūrdhanya (Retroflex)
                4 = Dantya (Dental)
                5 = Oṣṭhya (Labial)
- Bits [9:6]  : Prayatna (Manner of articulation):
                Bit 6 = Spṛṣṭa (Stop / Plosive)
                Bit 7 = Mahāprāṇa (Aspirated)
                Bit 8 = Ghoṣa (Voiced)
                Bit 9 = Anunāsika (Nasal)
- Bits [13:10]: Svara & Length:
                10-11 = Hrasva / Dīrgha / Pluta
                12-13 = Udātta / Anudātta / Svarita
- Bits [15:14]: Modifiers:
                Bit 14 = Palatalized modifier (e.g. Ukrainian soft consonant [ь])
                Bit 15 = Diphthong / Extension
"""

from dataclasses import dataclass
from typing import Dict, Optional, Tuple

# Bit constants
FLAG_VOWEL          = 1 << 0

# Sthāna (Place of Articulation) Bits [5:1]
STHANA_MASK         = 0x003E
STHANA_NONE         = 0 << 1
STHANA_KANTHYA      = 1 << 1  # Velar / Guttural (a, k, kh, g, gh, ṅ, h)
STHANA_TALAVYA      = 2 << 1  # Palatal (i, c, ch, j, jh, ñ, y, ś)
STHANA_MURDHANYA    = 3 << 1  # Retroflex (ṛ, ṭ, ṭh, ḍ, ḍh, ṇ, r, ṣ)
STHANA_DANTYA       = 4 << 1  # Dental (ḷ, t, th, d, dh, n, l, s)
STHANA_OSHTHYA      = 5 << 1  # Labial (u, p, ph, b, bh, m, v)

# Prayatna (Manner of Articulation) Bits [9:6]
PRAYATNA_MASK       = 0x03C0
PRAYATNA_SPRSTA     = 1 << 6  # Stop (k, c, ṭ, t, p...)
PRAYATNA_MAHAPRANA  = 1 << 7  # Aspirate (kh, gh, ch, jh...)
PRAYATNA_GHOSHA     = 1 << 8  # Voiced (g, gh, j, jh, d, dh, b, bh, nasals, semivowels, vowels)
PRAYATNA_ANUNASIKA  = 1 << 9  # Nasal (ṅ, ñ, ṇ, n, m)

# Length / Svara Bits [13:10]
LEN_MASK            = 0x0C00
LEN_HRASVA          = 1 << 10
LEN_DIRGHA          = 2 << 10
LEN_PLUTA           = 3 << 10

ACCENT_MASK         = 0x3000
ACCENT_UDATTA       = 1 << 12
ACCENT_ANUDATTA     = 2 << 12
ACCENT_SVARITA      = 3 << 12

# Modifiers Bits [15:14]
MOD_PALATALIZED     = 1 << 14 # Ukrainian [ь] / Palatalized consonant
MOD_DIPHTHONG       = 1 << 15

@dataclass(frozen=True)
class PhonemeVector:
    symbol: str
    code: int

    @property
    def is_vowel(self) -> bool:
        return bool(self.code & FLAG_VOWEL)

    @property
    def is_consonant(self) -> bool:
        return not self.is_vowel

    @property
    def is_voiced(self) -> bool:
        return bool(self.code & PRAYATNA_GHOSHA)

    @property
    def is_aspirate(self) -> bool:
        return bool(self.code & PRAYATNA_MAHAPRANA)

    @property
    def is_nasal(self) -> bool:
        return bool(self.code & PRAYATNA_ANUNASIKA)

    @property
    def is_stop(self) -> bool:
        return bool(self.code & PRAYATNA_SPRSTA)

    @property
    def is_palatalized(self) -> bool:
        return bool(self.code & MOD_PALATALIZED)

    @property
    def sthana(self) -> int:
        return self.code & STHANA_MASK

    def is_savarna_with(self, other: "PhonemeVector") -> bool:
        """
        Sūtra 1.1.9: tulyāsyaprayatnaṁ savarṇam.
        Two sounds are savarṇa if they share the same sthāna and internal prayatna (sprsta & vowel).
        """
        same_sthana = (self.sthana == other.sthana) and (self.sthana != 0)
        same_prayatna = (bool(self.code & PRAYATNA_SPRSTA) == bool(other.code & PRAYATNA_SPRSTA)) and \
                         (self.is_vowel == other.is_vowel)
        return same_sthana and same_prayatna

    def with_voicing(self, voiced: bool = True) -> "PhonemeVector":
        new_code = (self.code | PRAYATNA_GHOSHA) if voiced else (self.code & ~PRAYATNA_GHOSHA)
        return PhonemeVector(self.symbol, new_code)

    def with_palatalization(self, palatalized: bool = True) -> "PhonemeVector":
        new_code = (self.code | MOD_PALATALIZED) if palatalized else (self.code & ~MOD_PALATALIZED)
        return PhonemeVector(self.symbol, new_code)


# Canonical Register of PVC-16 Sound Vectors
REGISTRY: Dict[str, PhonemeVector] = {
    # Vowels
    "a":  PhonemeVector("a",  FLAG_VOWEL | STHANA_KANTHYA   | PRAYATNA_GHOSHA | LEN_HRASVA),
    "A":  PhonemeVector("A",  FLAG_VOWEL | STHANA_KANTHYA   | PRAYATNA_GHOSHA | LEN_DIRGHA),
    "i":  PhonemeVector("i",  FLAG_VOWEL | STHANA_TALAVYA   | PRAYATNA_GHOSHA | LEN_HRASVA),
    "I":  PhonemeVector("I",  FLAG_VOWEL | STHANA_TALAVYA   | PRAYATNA_GHOSHA | LEN_DIRGHA),
    "u":  PhonemeVector("u",  FLAG_VOWEL | STHANA_OSHTHYA   | PRAYATNA_GHOSHA | LEN_HRASVA),
    "U":  PhonemeVector("U",  FLAG_VOWEL | STHANA_OSHTHYA   | PRAYATNA_GHOSHA | LEN_DIRGHA),
    "f":  PhonemeVector("f",  FLAG_VOWEL | STHANA_MURDHANYA | PRAYATNA_GHOSHA | LEN_HRASVA), # ṛ
    "x":  PhonemeVector("x",  FLAG_VOWEL | STHANA_DANTYA    | PRAYATNA_GHOSHA | LEN_HRASVA), # ḷ

    # Velars (Kavarga)
    "k":  PhonemeVector("k",  STHANA_KANTHYA | PRAYATNA_SPRSTA),
    "K":  PhonemeVector("K",  STHANA_KANTHYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA),
    "g":  PhonemeVector("g",  STHANA_KANTHYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA),
    "G":  PhonemeVector("G",  STHANA_KANTHYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA | PRAYATNA_GHOSHA),
    "N":  PhonemeVector("N",  STHANA_KANTHYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | PRAYATNA_ANUNASIKA),

    # Palatals (Cavarga)
    "c":  PhonemeVector("c",  STHANA_TALAVYA | PRAYATNA_SPRSTA),
    "C":  PhonemeVector("C",  STHANA_TALAVYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA),
    "j":  PhonemeVector("j",  STHANA_TALAVYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA),
    "J":  PhonemeVector("J",  STHANA_TALAVYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA | PRAYATNA_GHOSHA),
    "Y":  PhonemeVector("Y",  STHANA_TALAVYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | PRAYATNA_ANUNASIKA),

    # Retroflexes (Ṭavarga)
    "w":  PhonemeVector("w",  STHANA_MURDHANYA | PRAYATNA_SPRSTA),
    "W":  PhonemeVector("W",  STHANA_MURDHANYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA),
    "q":  PhonemeVector("q",  STHANA_MURDHANYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA),
    "Q":  PhonemeVector("Q",  STHANA_MURDHANYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA | PRAYATNA_GHOSHA),
    "R":  PhonemeVector("R",  STHANA_MURDHANYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | PRAYATNA_ANUNASIKA),

    # Dentals (Tavarga)
    "t":  PhonemeVector("t",  STHANA_DANTYA  | PRAYATNA_SPRSTA),
    "T":  PhonemeVector("T",  STHANA_DANTYA  | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA),
    "d":  PhonemeVector("d",  STHANA_DANTYA  | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA),
    "D":  PhonemeVector("D",  STHANA_DANTYA  | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA | PRAYATNA_GHOSHA),
    "n":  PhonemeVector("n",  STHANA_DANTYA  | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | PRAYATNA_ANUNASIKA),

    # Labials (Pavarga)
    "p":  PhonemeVector("p",  STHANA_OSHTHYA | PRAYATNA_SPRSTA),
    "P":  PhonemeVector("P",  STHANA_OSHTHYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA),
    "b":  PhonemeVector("b",  STHANA_OSHTHYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA),
    "B":  PhonemeVector("B",  STHANA_OSHTHYA | PRAYATNA_SPRSTA | PRAYATNA_MAHAPRANA | PRAYATNA_GHOSHA),
    "m":  PhonemeVector("m",  STHANA_OSHTHYA | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | PRAYATNA_ANUNASIKA),

    # Ukrainian Extensions
    "t_soft": PhonemeVector("t_soft", STHANA_DANTYA  | PRAYATNA_SPRSTA | MOD_PALATALIZED),
    "d_soft": PhonemeVector("d_soft", STHANA_DANTYA  | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | MOD_PALATALIZED),
    "n_soft": PhonemeVector("n_soft", STHANA_DANTYA  | PRAYATNA_SPRSTA | PRAYATNA_GHOSHA | PRAYATNA_ANUNASIKA | MOD_PALATALIZED),
}

def get_phoneme(sym: str) -> Optional[PhonemeVector]:
    return REGISTRY.get(sym)
