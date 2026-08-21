"""
64-Bit Pratyāhāra Bitmask Module (UPC-8 / Śiva Sūtras)
======================================================
Canonical sounds span exactly 42 unique codes (0x00 to 0x29).
Any pratyāhāra S ⊆ {0, ..., 41} is encoded as a single 64-bit unsigned integer mask:
    Mask(S) = ∑_{c ∈ S} 2^c
"""

from typing import Dict, List, Set, Tuple, Union

# 42 Canonical sounds across 14 Śiva Sūtras (SLP1 transliteration)
CANONICAL_SOUNDS: List[str] = [
    # Sutras 1-4: Vowels (ac) -> bits 0..8 (9 sounds)
    "a", "i", "u", "f", "x", "e", "o", "E", "O",
    # Sutras 5-6: Semivowels + initial h -> bits 9..13 (5 sounds)
    "h", "y", "v", "r", "l",
    # Sutra 7: Nasals (ña-ma-ṅa-ṇa-na) -> bits 14..18 (5 sounds)
    "Y", "m", "N", "R", "n",
    # Sutras 8-9: Voiced Aspirated Stops -> bits 19..23 (5 sounds)
    "J", "B", "G", "Q", "D",
    # Sutra 10: Voiced Unaspirated Stops -> bits 24..28 (5 sounds)
    "j", "b", "g", "q", "d",
    # Sutra 11: Voiceless Aspirated Stops + Voiceless Palatal/Retroflex/Dental -> bits 29..36 (8 sounds)
    "K", "P", "C", "W", "T", "c", "w", "t",
    # Sutra 12: Voiceless Velar/Labial Stops -> bits 37..38 (2 sounds)
    "k", "p",
    # Sutra 13: Sibilants (Sar) -> bits 39..41 (3 sounds)
    "S", "z", "s",
]

assert len(CANONICAL_SOUNDS) == 42, f"Expected 42 unique sounds, got {len(CANONICAL_SOUNDS)}"

# Mappings
SOUND_TO_BIT: Dict[str, int] = {sound: i for i, sound in enumerate(CANONICAL_SOUNDS)}
BIT_TO_SOUND: Dict[int, str] = {i: sound for i, sound in enumerate(CANONICAL_SOUNDS)}

# Classical 42 Pratyāhāras defined by sound lists
PRATYAHARA_DEFINITIONS: Dict[str, List[str]] = {
    "ac":   ["a", "i", "u", "f", "x", "e", "o", "E", "O"],
    "ak":   ["a", "i", "u", "f", "x"],
    "ik":   ["i", "u", "f", "x"],
    "uk":   ["u", "f", "x"],
    "eN":   ["e", "o"],
    "ec":   ["e", "o", "E", "O"],
    "Ec":   ["E", "O"],
    "al":   CANONICAL_SOUNDS,  # All 42 sounds
    "hal":  CANONICAL_SOUNDS[9:],  # 33 consonants (h + semivowels + stops + sibilants)
    "val":  CANONICAL_SOUNDS[10:], # All consonants except initial h (32 consonants)
    "ral":  CANONICAL_SOUNDS[12:], # All consonants from r onwards (30 consonants)
    "Jal":  ["J", "B", "G", "Q", "D", "j", "b", "g", "q", "d",
             "K", "P", "C", "W", "T", "c", "w", "t", "k", "p", "S", "z", "s", "h"],
    "Sal":  ["S", "z", "s", "h"],
    "Sar":  ["S", "z", "s"],
    "yar":  CANONICAL_SOUNDS[10:42],
    "yay":  ["y", "v", "r", "l", "Y", "m", "N", "R", "n", "J", "B", "G", "Q", "D",
             "j", "b", "g", "q", "d", "K", "P", "C", "W", "T", "c", "w", "t", "k", "p"],
    "yaR":  ["y", "v", "r", "l"],
    "yam":  ["y", "v", "r", "l", "Y", "m", "N", "R", "n"],
    "yaY":  ["y", "v", "r", "l", "Y", "m", "N", "R", "n", "J", "B"],
    "vaw":  ["v", "r"],
    "may":  ["m", "N", "R", "n", "J", "B", "G", "Q", "D", "j", "b", "g", "q", "d",
             "K", "P", "C", "W", "T", "c", "w", "t", "k", "p"],
    "am":   ["a", "i", "u", "f", "x", "e", "o", "E", "O", "h", "y", "v", "r", "l",
             "Y", "m", "N", "R", "n"],
    "aw":   ["a", "i", "u", "f", "x", "e", "o", "E", "O", "h", "y", "v", "r"],
    "iR":   ["i", "u", "f", "x", "e", "o", "E", "O", "h", "y", "v", "r", "l"],
    "aR":   ["a", "i", "u"],
    "eR":   ["e", "o"],
    "nam":  ["N", "R", "n"],
    "JaS":  ["J", "B", "G", "Q", "D", "j", "b", "g", "q", "d"],
    "jaS":  ["j", "b", "g", "q", "d"],
    "baS":  ["b", "g", "q", "d"],
    "Jaz":  ["J", "B", "G", "Q", "D"],
    "Baz":  ["B", "G", "Q", "D"],
    "Jay":  ["J", "B", "G", "Q", "D", "j", "b", "g", "q", "d", "K", "P", "C", "W",
             "T", "c", "w", "t", "k", "p"],
    "Kay":  ["K", "P", "C", "W", "T", "c", "w", "t", "k", "p"],
    "xay":  ["K", "P", "C", "W", "T", "c", "w", "t"],
    "car":  ["c", "w", "t", "k", "p", "S", "z", "s"],
    "cav":  ["c", "w", "t"],
    "caw":  ["c", "w"],
    "Kar":  ["K", "P", "C", "W", "T", "c", "w", "t", "k", "p", "S", "z", "s"],
    "Jar":  ["J", "B", "G", "Q", "D", "j", "b", "g", "q", "d", "K", "P", "C", "W",
             "T", "c", "w", "t", "k", "p", "S", "z", "s"],
    "haS":  ["h", "y", "v", "r", "l", "Y", "m", "N", "R", "n", "J", "B", "G", "Q",
             "D", "j", "b", "g", "q", "d"],
}

def sounds_to_mask(sounds: List[str]) -> int:
    mask = 0
    for s in sounds:
        if s in SOUND_TO_BIT:
            mask |= (1 << SOUND_TO_BIT[s])
    return mask

# Precomputed 64-bit masks
PRATYAHARA_MASKS: Dict[str, int] = {
    name: sounds_to_mask(sounds) for name, sounds in PRATYAHARA_DEFINITIONS.items()
}

MASK_AL: int = (1 << 42) - 1 # 0x000003FFFFFFFFFF
MASK_AC: int = PRATYAHARA_MASKS["ac"] # 0x00000000000001FF
MASK_HAL: int = PRATYAHARA_MASKS["hal"] # 0x000003FFFFFFFFE00

def get_mask(name_or_mask: Union[str, int]) -> int:
    if isinstance(name_or_mask, int):
        return name_or_mask & MASK_AL
    if name_or_mask in PRATYAHARA_MASKS:
        return PRATYAHARA_MASKS[name_or_mask]
    raise KeyError(f"Unknown pratyahara: {name_or_mask}")

def mask_intersect(m1: int, m2: int) -> int:
    return m1 & m2

def mask_union(m1: int, m2: int) -> int:
    return (m1 | m2) & MASK_AL

def mask_diff(m1: int, m2: int) -> int:
    return m1 & (~m2) & MASK_AL

def mask_complement(m: int) -> int:
    return (~m) & MASK_AL

def mask_subset(m1: int, m2: int) -> bool:
    return (m1 & (~m2) & MASK_AL) == 0

def is_member(sound_code: int, mask: int) -> bool:
    if 0 <= sound_code < 42:
        return bool(mask & (1 << sound_code))
    return False

def mask_to_sounds(mask: int) -> List[str]:
    return [BIT_TO_SOUND[b] for b in range(42) if (mask & (1 << b))]
