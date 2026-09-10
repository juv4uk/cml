// Ad-hoc, not part of the crate's normal test suite: measures real wall-
// clock numbers on this host for three questions the owner asked directly
// (AOT compile-time cost, compiled-code execution speed vs. running the
// same source through the my-lisp interpreter, and FPGA-target program
// size) -- not fabricated, not asserted as thresholds, printed as-measured.
//
// Every number here is machine-specific (this host, this build, cold/warm
// cache state at run time) -- treat it as one data point, not a universal
// performance claim. Run with:
//   cargo run --release --example bench_aot_vs_interpreter -- /path/to/my-lisp
//
// The my-lisp binary path is required (no default guessed) so this never
// silently benchmarks against the wrong build.

use cml::build::{compile_c_to_executable, emit_c, front_end_to_ir};
use cml::compiler::Compiler;
use cml::lower;
use cml::parser;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

struct Fixture {
    name: &'static str,
    source: &'static str,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "trivial-one-shot",
        source: "(+ 1 2)",
    },
    Fixture {
        // Compiled via cc -O0 (build.rs's compile_c_to_executable) -- no
        // tail-call optimization, so this depth is chosen to stay well
        // within a default C stack rather than to be maximal.
        name: "self-recursive-count-5000",
        source: "(def count-down (lambda (n) (cond ((eq n 0) 0) (t (count-down (- n 1)))))) (count-down 5000)",
    },
    Fixture {
        name: "length-map-over-list",
        source: "(def length-onto (lambda (x acc) (cond ((atom x) acc) (t (length-onto (cdr x) (+ acc 1)))))) (def length (lambda (x) (length-onto x 0))) (def map (lambda (f xs) (cond ((atom xs) ()) (t (cons (f (car xs)) (map f (cdr xs))))))) (length (map (lambda (x) (+ x 1)) (quote (1 2 3 4 5 6 7 8 9 10))))",
    },
];

fn time_it<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

fn bench_c_backend(source: &str, tmp_dir: &Path, tag: &str) -> (Duration, Duration, String) {
    let (ir, compile_ir_time) = time_it(|| front_end_to_ir(source).expect("front_end_to_ir"));
    let c_source = emit_c(&ir).expect("emit_c");
    let bin_path = tmp_dir.join(format!("cml_bench_{tag}"));
    let (_, codegen_time) = time_it(|| compile_c_to_executable(&c_source, &bin_path).expect("cc"));
    let compile_total = compile_ir_time + codegen_time;

    let (output, run_time) = time_it(|| {
        Command::new(&bin_path)
            .output()
            .expect("run compiled binary")
    });
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (compile_total, run_time, stdout)
}

fn bench_fpga_instruction_count(source: &str) -> Result<usize, String> {
    let expressions = parser::parse(source).map_err(|e| e.to_string())?;
    let program = lower::lower_program(&expressions).map_err(|e| e.to_string())?;
    let mut compiler = Compiler::new();
    let asm = compiler.compile(&program).map_err(|e| e.to_string())?;
    // Count non-empty, non-comment, non-label lines as instructions -- the
    // same rough convention used in this repo's own evidence notes (e.g.
    // "218 instructions" in evidence/length/).
    let count = asm
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with(';') && !line.ends_with(':'))
        .count();
    Ok(count)
}

fn bench_interpreter(
    my_lisp_bin: &str,
    source: &str,
    tmp_dir: &Path,
    tag: &str,
) -> (Duration, String) {
    let src_path = tmp_dir.join(format!("cml_bench_{tag}.my"));
    std::fs::write(&src_path, source).expect("write source");
    let (output, run_time) = time_it(|| {
        Command::new(my_lisp_bin)
            .arg(&src_path)
            .output()
            .expect("run my-lisp interpreter")
    });
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (run_time, stdout)
}

fn main() {
    let my_lisp_bin = std::env::args()
        .nth(1)
        .expect("usage: bench_aot_vs_interpreter <path-to-my-lisp-binary>");
    let tmp_dir = std::env::temp_dir();

    println!("# cml AOT vs my-lisp interpreter -- real measurements on this host");
    println!("# Machine-specific data points, not a universal performance claim.\n");

    for fixture in FIXTURES {
        println!("## {}", fixture.name);
        println!("source: {}", fixture.source);

        let (c_compile_time, c_run_time, c_stdout) =
            bench_c_backend(fixture.source, &tmp_dir, fixture.name);
        let (interp_time, interp_stdout) =
            bench_interpreter(&my_lisp_bin, fixture.source, &tmp_dir, fixture.name);

        let fpga_instrs = bench_fpga_instruction_count(fixture.source);

        println!(
            "  C backend:    compile={:>8.3}ms  run={:>8.3}ms  result={:?}",
            c_compile_time.as_secs_f64() * 1000.0,
            c_run_time.as_secs_f64() * 1000.0,
            c_stdout
        );
        println!(
            "  interpreter:  run={:>8.3}ms  result={:?}",
            interp_time.as_secs_f64() * 1000.0,
            interp_stdout
        );
        match fpga_instrs {
            Ok(n) => {
                println!("  fpga-lisp asm: {n} instructions (static size, not timed execution)")
            }
            Err(e) => println!("  fpga-lisp asm: unsupported on this fixture ({e})"),
        }

        let c_total = c_compile_time + c_run_time;
        if interp_time > c_total {
            println!(
                "  => AOT (compile+run) was faster by {:.3}ms on this run",
                (interp_time - c_total).as_secs_f64() * 1000.0
            );
        } else {
            println!(
                "  => interpreter alone was faster by {:.3}ms on this run (compile overhead not recouped)",
                (c_total - interp_time).as_secs_f64() * 1000.0
            );
        }

        // The realistic host-dispatch shape: compile ONCE, then the game
        // process calls the same compiled binary/host-primitive repeatedly
        // across a session. This is the case where AOT's one-time compile
        // cost gets amortized -- measure it directly instead of assuming.
        const REPEATS: u32 = 200;
        let bin_path = tmp_dir.join(format!("cml_bench_{}", fixture.name));
        let (_, repeated_c_run_time) = time_it(|| {
            for _ in 0..REPEATS {
                Command::new(&bin_path)
                    .output()
                    .expect("run compiled binary");
            }
        });
        let src_path = tmp_dir.join(format!("cml_bench_{}.my", fixture.name));
        let (_, repeated_interp_time) = time_it(|| {
            for _ in 0..REPEATS {
                Command::new(&my_lisp_bin)
                    .arg(&src_path)
                    .output()
                    .expect("run interpreter");
            }
        });
        println!(
            "  {REPEATS}x calls (compile-once amortized): compiled={:.3}ms total ({:.4}ms/call)  interpreter={:.3}ms total ({:.4}ms/call)",
            repeated_c_run_time.as_secs_f64() * 1000.0,
            repeated_c_run_time.as_secs_f64() * 1000.0 / REPEATS as f64,
            repeated_interp_time.as_secs_f64() * 1000.0,
            repeated_interp_time.as_secs_f64() * 1000.0 / REPEATS as f64,
        );
        let breakeven = c_compile_time.as_secs_f64()
            / (repeated_interp_time.as_secs_f64() / REPEATS as f64
                - repeated_c_run_time.as_secs_f64() / REPEATS as f64)
                .max(f64::EPSILON);
        if breakeven.is_finite() && breakeven > 0.0 {
            println!(
                "  => break-even at ~{:.0} calls (compile cost amortized past that many invocations)",
                breakeven
            );
        }
        println!();
    }
}
