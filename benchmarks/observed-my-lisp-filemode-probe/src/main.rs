use std::rc::Rc;
use std::time::Instant;

use my_lisp::{parse, Environment, Session, Value, eval_parsed_expressions};

fn elapsed_ns(start: Instant) -> u128 {
    start.elapsed().as_nanos()
}

fn main() {
    my_lisp_host::install();

    let total_started = Instant::now();
    let mut session = Session {
        environment: Environment::root(),
    };

    let bootstrap_started = Instant::now();
    my_lisp::load_macro_library(&mut session).expect("observed PR #317 macro bootstrap");
    my_lisp::load_core_library(&mut session).expect("observed PR #317 core bootstrap");
    my_lisp::load_time_library(&mut session).expect("observed PR #317 time bootstrap");
    my_lisp::load_process_library(&mut session).expect("observed PR #317 process bootstrap");
    my_lisp::load_fs_library(&mut session).expect("observed PR #317 fs bootstrap");
    let bootstrap_ns = elapsed_ns(bootstrap_started);

    session.environment.define(
        "*argv*",
        Value::list([Value::String(Rc::from("--check"))]),
    );

    std::env::set_current_dir("observed-my-lisp")
        .expect("observed-my-lisp checkout must exist beside CML checkout");
    let script_path = "scripts/generate-meta-semantic-registry.lisp";

    let read_started = Instant::now();
    let source = std::fs::read_to_string(script_path).expect("read observed PR #317 Lisp generator");
    let read_ns = elapsed_ns(read_started);

    let parse_started = Instant::now();
    let ast = parse(&source).expect("parse observed PR #317 Lisp generator");
    let parse_ns = elapsed_ns(parse_started);
    let top_level_forms = ast.len();

    let eval_started = Instant::now();
    let result = eval_parsed_expressions(&ast, &mut session)
        .expect("evaluate observed PR #317 Lisp generator");
    let eval_ns = elapsed_ns(eval_started);

    for line in result.output {
        eprintln!("observed-script-output: {line}");
    }

    let observed_sha = std::env::var("MY_LISP_OBSERVED_SHA").unwrap_or_else(|_| "unknown".into());
    let measured_total_ns = read_ns + parse_ns + eval_ns;

    println!(
        "((kind . cml-observed-my-lisp-filemode-profile) \
         (upstream-channel . pr-317-head) \
         (my-lisp-sha . \"{observed_sha}\") \
         (script . \"{script_path}\") \
         (source-bytes . {}) \
         (top-level-forms . {top_level_forms}) \
         (bootstrap-ns . {bootstrap_ns}) \
         (read-ns . {read_ns}) \
         (parse-ns . {parse_ns}) \
         (eval-ns . {eval_ns}) \
         (measured-filemode-ns . {measured_total_ns}) \
         (process-total-ns . {}))",
        source.len(),
        elapsed_ns(total_started)
    );
}
