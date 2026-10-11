//! Mechanically project ONLY exact-width compiler candidate coordinates from
//! the SENS-owned fixture corpus. This generator supplies ZERO semantic roles:
//! the live SENS executable law decides those at runtime. Unlike historical
//! CML build.rs it does not produce byte dispatch tables or invoke old SIDs.
use std::{collections::BTreeSet, env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest path"));
    let authority = manifest.join("../../external/sens/contracts/compiler-nucleus-identity-corpus-v1.tsv");
    println!("cargo:rerun-if-changed={}", authority.display());
    let source = fs::read_to_string(&authority)
        .expect("BLOCK: verified exact SENS compiler identity corpus missing");
    let mut unique = BTreeSet::new();
    let mut rows = Vec::new();
    let mut d3 = 0;
    let mut d4 = 0;
    for line in source.lines().map(str::trim).filter(|x| !x.is_empty() && !x.starts_with('#')) {
        let parts: Vec<_> = line.split('\t').collect();
        assert!(parts.len() == 3, "BLOCK: malformed SENS compiler corpus TSV");
        let (fixture, domain, bits) = (parts[0], parts[1], parts[2]);
        let width = match domain {
            "D3" => { d3 += 1; 3 },
            "D4" => { d4 += 1; 4 },
            _ => panic!("BLOCK: unexpected semantic domain (D3/D4 only)"),
        };
        assert!(bits.len() == width && bits.bytes().all(|x| x == b'0' || x == b'1'),
            "BLOCK: a compiler coordinate must have its exact source width");
        assert!(fixture == format!("nucleus-{}-{bits}", domain.to_ascii_lowercase()),
            "BLOCK: mismatched SENS fixture and coordinate");
        assert!(unique.insert((domain.to_owned(), bits.to_owned())),
            "BLOCK: duplicate exact compiler identity");
        rows.push((fixture.to_owned(), domain.to_owned(), bits.to_owned()));
    }
    assert!(rows.len() == 9 && d3 == 7 && d4 == 2,
        "BLOCK: SENS-owned exact compiler closure changed (expected D3:7 D4:2)");
    let mut generated = String::from("// GENERATED FROM SENS COORDINATES ONLY; NO SEMANTIC ROLES.\n");
    generated.push_str("pub const COMPILER_CANDIDATE_COORDINATES: &[(&str, &str, &str)] = &[\n");
    for (fixture, domain, bits) in &rows {
        generated.push_str(&format!("    ({fixture:?}, {domain:?}, {bits:?}),\n"));
    }
    generated.push_str("];\n");
    let output = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"))
        .join("sens_compiler_candidates.rs");
    fs::write(&output, generated).expect("cannot write verified shape-only projection");
}
