//! C exact-Q representation baseline for cml#563.
//!
//! This test instruments the *existing generated C runtime* without changing
//! production semantics. It keeps CBackend's runtime/functions verbatim,
//! replaces only the generated program entrypoint with a measurement-only
//! main in the same translation unit, compiles it with the system C compiler,
//! and parses one machine-readable result row.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cml::c_backend::CBackend;
use cml::ir::Ir;

fn gcc_command() -> Command {
    let mut cmd = Command::new("gcc");
    if std::env::var("C_INCLUDE_PATH").is_err()
        && std::path::Path::new("/var/guix/profiles/shared/guix-profile/include").exists()
    {
        cmd.env(
            "C_INCLUDE_PATH",
            "/var/guix/profiles/shared/guix-profile/include",
        );
    }
    cmd
}

fn unique_base() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "cml_exact_q_c_baseline_{}_{}",
        std::process::id(),
        nonce
    ))
}

fn measurement_source() -> String {
    let generated = CBackend::new()
        .compile_program(&[Ir::Int(0)])
        .expect("trivial C program must compile");

    let main_marker = "
int main(void) {";
    let runtime_and_functions = generated
        .split_once(main_marker)
        .map(|(prefix, _)| prefix)
        .expect("generated C must contain the documented int main(void) entrypoint");

    format!(
        r#"{runtime_and_functions}

int main(void) {{
    const size_t value_size = sizeof(Value);
    const size_t pointer_size = sizeof(void *);
    const size_t long_size = sizeof(long);

    size_t before = cml_bytes_allocated;
    Value *int_one = mk_int(1);
    size_t int_bytes = cml_bytes_allocated - before;

    before = cml_bytes_allocated;
    Value *rat_half = mk_rational(1, 2);
    size_t rat_half_bytes = cml_bytes_allocated - before;

    before = cml_bytes_allocated;
    Value *rat_one = mk_rational(1, 1);
    size_t rat_one_bytes = cml_bytes_allocated - before;

    before = cml_bytes_allocated;
    Value *rat_large = mk_rational(LONG_MAX, LONG_MAX - 1);
    size_t rat_large_bytes = cml_bytes_allocated - before;

    before = cml_bytes_allocated;
    Value *int_as_rational = to_rational(int_one);
    size_t int_to_rat_bytes = cml_bytes_allocated - before;

    before = cml_bytes_allocated;
    Value *mixed_sum = v_add(int_one, rat_half);
    size_t mixed_add_bytes = cml_bytes_allocated - before;

    before = cml_bytes_allocated;
    Value *chain_a = mk_rational(1, 2);
    Value *chain_b = mk_rational(1, 3);
    Value *chain_sum = v_rat_add(chain_a, chain_b);
    size_t add_chain_bytes = cml_bytes_allocated - before;

    printf(
        "CML-EXACT-Q-C-BASELINE "
        "value_size=%zu pointer_size=%zu long_size=%zu stdc_version=%ld "
        "int_bytes=%zu rat_half_bytes=%zu "
        "rat_one_bytes=%zu rat_large_bytes=%zu "
        "int_to_rat_bytes=%zu mixed_add_bytes=%zu add_chain_bytes=%zu "
        "int_tag=%d rat_one_tag=%d rat_one_den=%ld "
        "print_int=",
        value_size,
        pointer_size,
        long_size,
        (long)__STDC_VERSION__,
        int_bytes,
        rat_half_bytes,
        rat_one_bytes,
        rat_large_bytes,
        int_to_rat_bytes,
        mixed_add_bytes,
        add_chain_bytes,
        (int)int_one->tag,
        (int)rat_one->tag,
        rat_one->u.rat.den
    );
    print_value(int_one);
    printf(" print_rat_one=");
    print_value(rat_one);
    printf(" eq_int_rat=");
    print_value(v_eq(int_one, rat_one));
    printf(" int_as_rat=");
    print_value(int_as_rational);
    printf(" mixed_sum=");
    print_value(mixed_sum);
    printf(" chain_sum=");
    print_value(chain_sum);
    printf("\n");

    /* Keep these live and prove normalization shape while avoiding optimizer-
     * dependent assumptions about unused allocations. */
    if (rat_half->u.rat.num != 1 || rat_half->u.rat.den != 2) return 91;
    if (rat_large->u.rat.den <= 0) return 92;
    return 0;
}}
"#
    )
}

#[derive(Debug)]
struct BaselineRow {
    value_size: usize,
    pointer_size: usize,
    long_size: usize,
    stdc_version: i64,
    int_bytes: usize,
    rat_half_bytes: usize,
    rat_one_bytes: usize,
    rat_large_bytes: usize,
    int_to_rat_bytes: usize,
    mixed_add_bytes: usize,
    add_chain_bytes: usize,
    int_tag: i32,
    rat_one_tag: i32,
    rat_one_den: i64,
    print_int: String,
    print_rat_one: String,
    eq_int_rat: String,
    int_as_rat: String,
    mixed_sum: String,
    chain_sum: String,
}

fn parse_row(stdout: &str) -> BaselineRow {
    let line = stdout.trim();
    let rest = line
        .strip_prefix("CML-EXACT-Q-C-BASELINE ")
        .expect("measurement row prefix");

    let mut value_size = None;
    let mut pointer_size = None;
    let mut long_size = None;
    let mut stdc_version = None;
    let mut int_bytes = None;
    let mut rat_half_bytes = None;
    let mut rat_one_bytes = None;
    let mut rat_large_bytes = None;
    let mut int_to_rat_bytes = None;
    let mut mixed_add_bytes = None;
    let mut add_chain_bytes = None;
    let mut int_tag = None;
    let mut rat_one_tag = None;
    let mut rat_one_den = None;
    let mut print_int = None;
    let mut print_rat_one = None;
    let mut eq_int_rat = None;
    let mut int_as_rat = None;
    let mut mixed_sum = None;
    let mut chain_sum = None;

    for field in rest.split_whitespace() {
        let (key, value) = field
            .split_once('=')
            .expect("every measurement field must be key=value");
        match key {
            "value_size" => value_size = Some(value.parse().expect("value_size usize")),
            "pointer_size" => pointer_size = Some(value.parse().expect("pointer_size usize")),
            "long_size" => long_size = Some(value.parse().expect("long_size usize")),
            "stdc_version" => stdc_version = Some(value.parse().expect("stdc_version i64")),
            "int_bytes" => int_bytes = Some(value.parse().expect("int_bytes usize")),
            "rat_half_bytes" => {
                rat_half_bytes = Some(value.parse().expect("rat_half_bytes usize"))
            }
            "rat_one_bytes" => {
                rat_one_bytes = Some(value.parse().expect("rat_one_bytes usize"))
            }
            "rat_large_bytes" => {
                rat_large_bytes = Some(value.parse().expect("rat_large_bytes usize"))
            }
            "int_to_rat_bytes" => {
                int_to_rat_bytes = Some(value.parse().expect("int_to_rat_bytes usize"))
            }
            "mixed_add_bytes" => {
                mixed_add_bytes = Some(value.parse().expect("mixed_add_bytes usize"))
            }
            "add_chain_bytes" => {
                add_chain_bytes = Some(value.parse().expect("add_chain_bytes usize"))
            }
            "int_tag" => int_tag = Some(value.parse().expect("int_tag i32")),
            "rat_one_tag" => rat_one_tag = Some(value.parse().expect("rat_one_tag i32")),
            "rat_one_den" => rat_one_den = Some(value.parse().expect("rat_one_den i64")),
            "print_int" => print_int = Some(value.to_string()),
            "print_rat_one" => print_rat_one = Some(value.to_string()),
            "eq_int_rat" => eq_int_rat = Some(value.to_string()),
            "int_as_rat" => int_as_rat = Some(value.to_string()),
            "mixed_sum" => mixed_sum = Some(value.to_string()),
            "chain_sum" => chain_sum = Some(value.to_string()),
            other => panic!("unknown measurement field {other:?}"),
        }
    }

    BaselineRow {
        value_size: value_size.expect("value_size"),
        pointer_size: pointer_size.expect("pointer_size"),
        long_size: long_size.expect("long_size"),
        stdc_version: stdc_version.expect("stdc_version"),
        int_bytes: int_bytes.expect("int_bytes"),
        rat_half_bytes: rat_half_bytes.expect("rat_half_bytes"),
        rat_one_bytes: rat_one_bytes.expect("rat_one_bytes"),
        rat_large_bytes: rat_large_bytes.expect("rat_large_bytes"),
        int_to_rat_bytes: int_to_rat_bytes.expect("int_to_rat_bytes"),
        mixed_add_bytes: mixed_add_bytes.expect("mixed_add_bytes"),
        add_chain_bytes: add_chain_bytes.expect("add_chain_bytes"),
        int_tag: int_tag.expect("int_tag"),
        rat_one_tag: rat_one_tag.expect("rat_one_tag"),
        rat_one_den: rat_one_den.expect("rat_one_den"),
        print_int: print_int.expect("print_int"),
        print_rat_one: print_rat_one.expect("print_rat_one"),
        eq_int_rat: eq_int_rat.expect("eq_int_rat"),
        int_as_rat: int_as_rat.expect("int_as_rat"),
        mixed_sum: mixed_sum.expect("mixed_sum"),
        chain_sum: chain_sum.expect("chain_sum"),
    }
}

#[test]
fn current_c_exact_q_representation_has_measured_heap_and_observer_baseline() {
    let source = measurement_source();
    let base = unique_base();
    let c_path = base.with_extension("c");
    let bin_path = base.with_extension("bin");

    fs::write(&c_path, &source).expect("write measurement C source");

    let compile = gcc_command()
        .arg("-std=c11")
        .arg("-O2")
        .arg(&c_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("run gcc");

    if !compile.status.success() {
        panic!(
            "gcc failed:\nSTDERR: {}\n--- generated measurement C ---\n{}",
            String::from_utf8_lossy(&compile.stderr),
            source
        );
    }

    let run = Command::new(&bin_path)
        .output()
        .expect("run C baseline witness");

    let _ = fs::remove_file(&c_path);
    let _ = fs::remove_file(&bin_path);

    assert!(
        run.status.success(),
        "C baseline witness failed: status={:?} stderr={}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8(run.stdout).expect("measurement stdout must be UTF-8");
    let row = parse_row(&stdout);

    assert!(row.value_size > 0, "Value must occupy storage");
    assert!(row.pointer_size > 0);
    assert!(row.long_size > 0);
    assert!(row.stdc_version >= 201112, "witness is compiled as C11 or newer");
    assert_eq!(row.int_bytes, row.value_size);
    assert_eq!(row.rat_half_bytes, row.value_size);
    assert_eq!(row.rat_one_bytes, row.value_size);
    assert_eq!(row.rat_large_bytes, row.value_size);
    assert_eq!(
        row.int_to_rat_bytes, row.value_size,
        "current INT -> exact-rational conversion allocates one Value"
    );
    assert_eq!(
        row.mixed_add_bytes,
        2 * row.value_size,
        "current INT + RATIONAL path allocates one conversion Value plus one result Value"
    );
    assert_eq!(
        row.add_chain_bytes,
        3 * row.value_size,
        "two exact-Q inputs plus one exact-Q result are three current Value allocations"
    );

    assert_ne!(
        row.int_tag, row.rat_one_tag,
        "current C runtime keeps denominator-1 rational and integer in distinct tag classes"
    );
    assert_eq!(row.rat_one_den, 1);

    assert_eq!(
        row.print_int, row.print_rat_one,
        "current printer intentionally renders 1 and 1/1 identically"
    );
    assert_eq!(row.print_int, "1");
    assert_eq!(
        row.eq_int_rat, "(0)",
        "current eq observer distinguishes TAG_INT from denominator-1 TAG_RATIONAL"
    );
    assert_eq!(row.int_as_rat, "1");
    assert_eq!(row.mixed_sum, "3/2");
    assert_eq!(row.chain_sum, "5/6");

    let compiler = gcc_command()
        .arg("--version")
        .output()
        .expect("query gcc version");
    let compiler_line = String::from_utf8_lossy(&compiler.stdout)
        .lines()
        .next()
        .unwrap_or("unknown")
        .replace(' ', "_");
    eprintln!("{}", stdout.trim());
    eprintln!(
        "CML-EXACT-Q-C-BASELINE-PROVENANCE compiler={} c_flags=-std=c11,-O2 rust_target_arch={} rust_target_os={}",
        compiler_line,
        std::env::consts::ARCH,
        std::env::consts::OS
    );
}
