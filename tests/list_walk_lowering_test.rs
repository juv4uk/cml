use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};

#[test]
fn simple_tail_recursive_list_walker_compiles_to_native_loop() {
    let source = r#"
      (def list-walk
        (lambda (bytes)
          (cond
            ((atom bytes) (eq bytes (quote ())))
            (t (list-walk (cdr bytes))))))
      (list-walk (quote (65 66 67)))
    "#;

    let expressions = parser::parse(source).expect("must parse");
    let program = lower::lower_program_with_tail_calls(&expressions).expect("must lower");
    println!("Lowered IR: {:#?}", program);
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("x86 freestanding must compile list walker");
    println!("=== GENERATED X86 ASSEMBLY ===\n{}\n==============================", assembly);

    assert!(
        assembly.contains("jmp .Ltcloop_"),
        "list walker must contain tail loop back-edge"
    );

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-list-walk-{nonce}"));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");

    let canonical_t = wsm_os_target::encode_symbol(wsm_os_target::SYMBOL_ID_MAX)
        .expect("canonical t symbol");

    std::fs::write(&source, &assembly).unwrap();
    std::fs::write(
        &harness,
        format!(
            r#"
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
extern uint64_t wsm_entry(void *);
int main(void) {{
    uint64_t res = wsm_entry(0);
    return (res == {canonical_t}ULL) ? 0 : 1;
}}
"#
        ),
    )
    .unwrap();

    let nucleus_path = cml::x86_freestanding::resolve_nucleus_asm_path()
        .expect("resolve nucleus.s for freestanding list witness");
    let linked = std::process::Command::new("cc")
        .arg(&harness)
        .arg(&source)
        .arg(&nucleus_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile and link with cc");

    assert!(
        linked.status.success(),
        "linking failed: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = std::process::Command::new(&executable)
        .output()
        .expect("execute native list-walk");
    println!("Run stdout: {}", String::from_utf8_lossy(&run.stdout));
    println!("Run stderr: {}", String::from_utf8_lossy(&run.stderr));
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&harness);
    let _ = std::fs::remove_file(&executable);

    assert_eq!(
        run.status.code(),
        Some(0),
        "native list-walk must successfully return canonical t!"
    );
}

#[test]
fn tail_recursive_list_walker_with_item_predicate() {
    let source = r#"
      (def my-not
        (lambda (v)
          (cond (v (quote ()))
                (t t))))

      (def is-even?
        (lambda (b)
          (cond
            ((eq (mod b 2) 0) t)
            (t (quote ())))))

      (def all-even?
        (lambda (bytes)
          (cond
            ((atom bytes) (eq bytes (quote ())))
            ((my-not (is-even? (car bytes))) (quote ()))
            (t (all-even? (cdr bytes))))))

      (all-even? (quote (2 4 6 8)))
    "#;

    let expressions = parser::parse(source).expect("must parse");
    let program = lower::lower_program_with_tail_calls(&expressions).expect("must lower");
    println!("Multi-def Lowered IR: {:#?}", program);
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .expect("x86 freestanding must compile multi-def list walker");

    assert!(
        assembly.contains("jmp .Ltcloop_"),
        "must contain tail loop back-edge"
    );

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-all-even-{nonce}"));
    let source = base.with_extension("s");
    let harness = base.with_extension("c");
    let executable = base.with_extension("bin");

    let canonical_t = wsm_os_target::encode_symbol(wsm_os_target::SYMBOL_ID_MAX)
        .expect("canonical t symbol");

    std::fs::write(&source, &assembly).unwrap();
    std::fs::write(
        &harness,
        format!(
            r#"
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
extern uint64_t wsm_entry(void *);
int main(void) {{
    uint64_t res = wsm_entry(0);
    return (res == {canonical_t}ULL) ? 0 : 1;
}}
"#
        ),
    )
    .unwrap();

    let nucleus_path = cml::x86_freestanding::resolve_nucleus_asm_path()
        .expect("resolve nucleus.s for freestanding list witness");
    let linked = std::process::Command::new("cc")
        .arg(&harness)
        .arg(&source)
        .arg(&nucleus_path)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile and link with cc");

    assert!(
        linked.status.success(),
        "linking failed: {}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let run = std::process::Command::new(&executable)
        .output()
        .expect("execute native all-even");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&harness);
    let _ = std::fs::remove_file(&executable);

    assert_eq!(
        run.status.code(),
        Some(0),
        "native all-even (with mod and multi-def calls) must return canonical t!"
    );
}



