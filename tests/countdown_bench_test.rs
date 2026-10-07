use cml::x86_freestanding::X86FreestandingBackend;
use cml::{lower, parser};
use std::fs;
use std::process::Command;

#[test]
fn honest_c_vs_lisp_benchmark() {
    let n: i64 = 10_000_000;

    // 1. C Implementation
    let c_source = format!(
        r#"
#include <stdint.h>
#include <stdio.h>
#include <time.h>

__attribute__((noinline))
uint64_t c_countdown(uint64_t n) {{
    while (n > 0) {{
        n = n - 1;
        asm volatile("" : "+r"(n));
    }}
    return n;
}}

int main() {{
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    uint64_t res = c_countdown({n}ULL);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double dt = (t1.tv_sec - t0.tv_sec) + (t1.tv_nsec - t0.tv_nsec) * 1e-9;
    printf("  Time: %.6f s (%.2f ns/iter)\n", dt, (dt / {n}.0) * 1e9);
    return 0;
}}
"#
    );
    fs::write("/tmp/c_bench.c", &c_source).unwrap();

    // Compile C with -O3
    Command::new("cc")
        .args(&["-O3", "/tmp/c_bench.c", "-o", "/tmp/c_bench_o3"])
        .status()
        .unwrap();

    // Compile C with -O0 (unoptimized)
    Command::new("cc")
        .args(&["-O0", "/tmp/c_bench.c", "-o", "/tmp/c_bench_o0"])
        .status()
        .unwrap();

    // 2. Lisp compiled via CML
    let lisp_source = format!(
        r#"
      (def countdown
        (lambda (n)
          (cond
            ((тотожне? n 0) 0)
            ((тотожне? n n) (countdown (- n 1))))))
      (countdown {n})
    "#
    );

    let expressions = parser::parse(&lisp_source).expect("parse");
    let program = lower::lower_program_with_tail_calls(&expressions).expect("lower");
    let assembly = X86FreestandingBackend::new()
        .compile_program(&program)
        .unwrap_or_else(|error| panic!("compile: {error:?}; lowered IR: {program:#?}"));

    fs::write("/tmp/lisp_count.s", &assembly).unwrap();

    let harness = format!(
        r#"
#include <stdint.h>
#include <stdio.h>
#include <time.h>

extern uint64_t wsm_entry(void *);

int main() {{
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    uint64_t res = wsm_entry(0);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    double dt = (t1.tv_sec - t0.tv_sec) + (t1.tv_nsec - t0.tv_nsec) * 1e-9;
    printf("  Time: %.6f s (%.2f ns/iter)\n", dt, (dt / {n}.0) * 1e9);
    return 0;
}}
"#
    );
    fs::write("/tmp/harness.c", &harness).unwrap();

    let nucleus_path = cml::x86_freestanding::resolve_nucleus_asm_path().unwrap();
    let status = Command::new("cc")
        .args(&[
            "-O3",
            "/tmp/harness.c",
            "/tmp/lisp_count.s",
            nucleus_path.to_str().unwrap(),
            "-o",
            "/tmp/lisp_bench",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    // 3. Measure C (-O3)
    println!("\n=== 1. C (gcc -O3, optimized native) ===");
    let out_c_o3 = Command::new("/tmp/c_bench_o3").output().unwrap();
    print!("{}", String::from_utf8_lossy(&out_c_o3.stdout));

    // 4. Measure C (-O0)
    println!("=== 2. C (gcc -O0, unoptimized baseline) ===");
    let out_c_o0 = Command::new("/tmp/c_bench_o0").output().unwrap();
    print!("{}", String::from_utf8_lossy(&out_c_o0.stdout));

    // 5. Measure Lisp (CML compiled native x86)
    println!("=== 3. Lisp (CML -> нативний x86-64 машинний код) ===");
    let out_lisp = Command::new("/tmp/lisp_bench").output().unwrap();
    print!("{}", String::from_utf8_lossy(&out_lisp.stdout));

    // Clean up
    let _ = fs::remove_file("/tmp/c_bench.c");
    let _ = fs::remove_file("/tmp/c_bench_o3");
    let _ = fs::remove_file("/tmp/c_bench_o0");
    let _ = fs::remove_file("/tmp/lisp_count.s");
    let _ = fs::remove_file("/tmp/harness.c");
    let _ = fs::remove_file("/tmp/lisp_bench");
}
