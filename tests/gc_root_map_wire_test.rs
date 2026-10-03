use cml::lower;
use cml::parser;
use cml::x86_freestanding::X86FreestandingBackend;
use cml::x86_freestanding_metadata::parse_gc_root_map_manifest;

#[test]
fn certified_comments_project_to_deterministic_machine_readable_records() {
    let expressions = parser::parse("((lambda (a b . rest) rest) 10 20 30 40 50)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();

    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("bounded variadic fixture must compile");

    assert_eq!(compiled.gc_root_maps.len(), 3);
    assert!(compiled.validate_gc_root_maps());

    for (expected_id, record) in compiled.gc_root_maps.iter().enumerate() {
        assert_eq!(record.id, expected_id);
        assert_eq!(record.return_label, format!(".Lgc_return_{expected_id}"));
        assert_eq!(record.allocator, "wsm_cons");
        assert_eq!(record.certificate_kind, "pack-rest-bounded");
        assert_eq!(record.register_roots, vec!["%rdx", "%rsi"]);
        assert!(!record.stack_offsets.is_empty());
        assert!(
            record
                .stack_offsets
                .iter()
                .all(|offset| offset % 8 == 0 && *offset < record.frame_bytes)
        );

        let comment = format!(
            "# GC_SAFEPOINT id={} kind={} allocator={} frame={} return_label={}",
            record.id,
            record.certificate_kind,
            record.allocator,
            record.frame_bytes,
            record.return_label
        );
        assert!(compiled.assembly.contains(&comment));

        let call_and_label = format!("    call {}\n{}:", record.allocator, record.return_label);
        assert!(
            compiled.assembly.contains(&call_and_label),
            "return label must denote the PC immediately after the certified allocation"
        );
    }

    let manifest = compiled.gc_root_map_manifest();
    let decoded = parse_gc_root_map_manifest(&manifest).expect("manifest must decode");
    assert_eq!(decoded, compiled.gc_root_maps);

    let replay = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("same fixture must recompile");
    assert_eq!(
        replay.gc_root_map_manifest(),
        manifest,
        "wire projection must be byte-deterministic"
    );
}

#[test]
fn absent_certificate_stays_absent_not_certified_empty() {
    let expressions = parser::parse("((lambda (x) x) 7)").unwrap();
    let program = lower::lower_program(&expressions).unwrap();

    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("non-allocating fixture must compile");

    assert!(compiled.gc_root_maps.is_empty());
    assert_eq!(compiled.gc_root_map_manifest(), "CML_GC_ROOT_MAP_V1\n");
    assert_eq!(
        parse_gc_root_map_manifest(&compiled.gc_root_map_manifest()).unwrap(),
        Vec::new()
    );
}

#[test]
fn manifest_parser_fails_closed_on_corrupt_root_metadata() {
    let good = concat!(
        "CML_GC_ROOT_MAP_V1\n",
        "site id=0 label=.Lgc_return_0 allocator=wsm_cons ",
        "kind=runtime-call-structured frame=16 stack=0,8 regs=%rdx,%rsi\n"
    );
    assert_eq!(parse_gc_root_map_manifest(good).unwrap().len(), 1);

    let duplicate = format!("{good}{}", good.lines().nth(1).unwrap()) + "\n";
    assert!(parse_gc_root_map_manifest(&duplicate).is_err());

    let bad_offset = concat!(
        "CML_GC_ROOT_MAP_V1\n",
        "site id=0 label=.Lgc_return_0 allocator=wsm_cons ",
        "kind=runtime-call-structured frame=16 stack=16 regs=%rdx,%rsi\n"
    );
    assert!(parse_gc_root_map_manifest(bad_offset).is_err());

    let bad_register = concat!(
        "CML_GC_ROOT_MAP_V1\n",
        "site id=0 label=.Lgc_return_0 allocator=wsm_cons ",
        "kind=runtime-call-structured frame=16 stack=0 regs=%rax\n"
    );
    assert!(parse_gc_root_map_manifest(bad_register).is_err());

    let wrong_label = concat!(
        "CML_GC_ROOT_MAP_V1\n",
        "site id=0 label=.Lgc_return_9 allocator=wsm_cons ",
        "kind=runtime-call-structured frame=16 stack=0 regs=%rdx\n"
    );
    assert!(parse_gc_root_map_manifest(wrong_label).is_err());

    let wrong_version = good.replacen("CML_GC_ROOT_MAP_V1", "CML_GC_ROOT_MAP_V2", 1);
    assert!(parse_gc_root_map_manifest(&wrong_version).is_err());
}


#[test]
fn quote_bounded_certificates_project_to_machine_readable_wire() {
    let expressions = parser::parse("(quote (A B C))").unwrap();
    let program = lower::lower_program(&expressions).unwrap();

    let compiled = X86FreestandingBackend::new()
        .compile_program_with_metadata(&program)
        .expect("quoted proper list must compile with metadata");

    let quote: Vec<_> = compiled
        .gc_root_maps
        .iter()
        .filter(|record| record.certificate_kind == "quote-bounded")
        .collect();

    assert_eq!(quote.len(), 3);
    assert!(compiled.validate_gc_root_maps());

    for record in quote {
        assert_eq!(record.allocator, "wsm_cons");
        assert_eq!(record.register_roots, vec!["%rdx", "%rsi"]);
        assert_eq!(record.stack_offsets.len(), 1);
        assert!(record.stack_offsets[0] < record.frame_bytes);
        let call_and_label = format!("    call wsm_cons\n{}:", record.return_label);
        assert!(compiled.assembly.contains(&call_and_label));
    }

    let manifest = compiled.gc_root_map_manifest();
    let decoded = parse_gc_root_map_manifest(&manifest).expect("quote manifest must decode");
    assert_eq!(decoded, compiled.gc_root_maps);
    assert_eq!(
        decoded
            .iter()
            .filter(|record| record.certificate_kind == "quote-bounded")
            .count(),
        3
    );
}
