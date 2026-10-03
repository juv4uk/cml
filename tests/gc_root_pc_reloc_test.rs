use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;
use std::io::Write;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_base(stem: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("cml-{stem}-{}-{nonce}", std::process::id()))
}

fn link_and_read_bindings(source: &str, text_base: &str, stem: &str) -> Vec<(u64, u64)> {
    let base = unique_base(stem);
    let s_path = base.with_extension("s");
    let o_path = base.with_extension("o");
    let elf_path = base.with_extension("elf");
    let bin_path = base.with_extension("bind.bin");

    let mut file = std::fs::File::create(&s_path).expect("create relocation witness assembly");
    file.write_all(source.as_bytes())
        .expect("write relocation witness assembly");
    file.flush().expect("flush relocation witness assembly");

    let assembled = Command::new("as")
        .arg("--64")
        .arg(&s_path)
        .arg("-o")
        .arg(&o_path)
        .output()
        .expect("run GNU as");
    assert!(
        assembled.status.success(),
        "GNU as failed: {}",
        String::from_utf8_lossy(&assembled.stderr)
    );

    let linked = Command::new("ld")
        .arg("-m")
        .arg("elf_x86_64")
        .arg("-e")
        .arg("wsm_entry")
        .arg(format!("-Ttext={text_base}"))
        .arg(&o_path)
        .arg("-o")
        .arg(&elf_path)
        .output()
        .expect("run GNU ld");
    assert!(
        linked.status.success(),
        "GNU ld failed: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let copied = Command::new("objcopy")
        .arg("-O")
        .arg("binary")
        .arg("--only-section=.wsm_gc_root_pc_bind")
        .arg(&elf_path)
        .arg(&bin_path)
        .output()
        .expect("run GNU objcopy");
    assert!(
        copied.status.success(),
        "objcopy failed: {}",
        String::from_utf8_lossy(&copied.stderr)
    );

    let bytes = std::fs::read(&bin_path).expect("read linked PC-binding section");
    assert_eq!(
        bytes.len() % 16,
        0,
        "binding section must contain u64 pairs"
    );
    let records = bytes
        .chunks_exact(16)
        .map(|chunk| {
            let id = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let pc = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
            (id, pc)
        })
        .collect::<Vec<_>>();

    let _ = std::fs::remove_file(s_path);
    let _ = std::fs::remove_file(o_path);
    let _ = std::fs::remove_file(elf_path);
    let _ = std::fs::remove_file(bin_path);
    records
}

#[test]
fn certified_return_labels_relocate_to_final_linked_pcs() {
    let expressions = parser::parse("((lambda (a b . rest) rest) 10 20 30 40 50)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("bounded variadic root-map witness must compile");

    assert_eq!(compiled.gc_root_maps.len(), 3);
    let mut assembly = compiled
        .assembly_with_gc_root_pc_bindings()
        .expect("valid compiler root maps must project relocation bindings");

    assembly.push_str("\n.text\n.globl wsm_cons\n.type wsm_cons,@function\nwsm_cons:\n    ret\n");

    let low = link_and_read_bindings(&assembly, "0x100000", "gc-pc-low");
    let high = link_and_read_bindings(&assembly, "0x200000", "gc-pc-high");

    assert_eq!(low.len(), compiled.gc_root_maps.len());
    assert_eq!(high.len(), compiled.gc_root_maps.len());

    for (index, ((low_id, low_pc), (high_id, high_pc))) in low.iter().zip(high.iter()).enumerate() {
        assert_eq!(*low_id, index as u64);
        assert_eq!(*high_id, index as u64);
        assert_ne!(*low_pc, 0);
        assert_ne!(*high_pc, 0);
        assert_eq!(
            high_pc - low_pc,
            0x100000,
            "final PC must follow linker placement automatically"
        );
    }

    let mut low_pcs = low.iter().map(|(_, pc)| *pc).collect::<Vec<_>>();
    low_pcs.sort_unstable();
    low_pcs.dedup();
    assert_eq!(
        low_pcs.len(),
        low.len(),
        "distinct certified sites must not alias one final-PC key"
    );
}

#[test]
fn absent_root_certificate_emits_no_pc_binding_section() {
    let expressions = parser::parse("((lambda (x) x) 7)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();
    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("non-allocating fixture must compile");

    assert!(compiled.gc_root_maps.is_empty());
    assert_eq!(
        compiled
            .assembly_with_gc_root_pc_bindings()
            .expect("absence is a valid non-safepoint state"),
        compiled.assembly
    );
}
