use std::time::Instant;

use my_lisp::{Environment, Session, eval_program};

fn elapsed_ns(start: Instant) -> u128 {
    start.elapsed().as_nanos()
}

fn timed_eval(label: &str, source: &str, session: &mut Session) -> u128 {
    let started = Instant::now();
    let _ = eval_program(source, session)
        .unwrap_or_else(|error| panic!("{label} probe failed: {error}"));
    elapsed_ns(started)
}

fn main() {
    my_lisp_host::install();

    let mut session = Session {
        environment: Environment::root(),
    };

    let bootstrap_started = Instant::now();
    my_lisp::load_macro_library(&mut session).expect("macro bootstrap");
    my_lisp::load_core_library(&mut session).expect("core bootstrap");
    my_lisp::load_time_library(&mut session).expect("time bootstrap");
    my_lisp::load_fs_library(&mut session).expect("fs + utf8 bootstrap");
    let bootstrap_ns = elapsed_ns(bootstrap_started);

    std::env::set_current_dir("observed-my-lisp")
        .expect("observed-my-lisp checkout must exist beside CML checkout");

    let path = "lib/surface/semantic-registry.lisp";

    let raw_read_started = Instant::now();
    let raw_bytes = std::fs::read(path).expect("raw mechanism read");
    let raw_read_ns = elapsed_ns(raw_read_started);
    let file_bytes = raw_bytes.len();
    drop(raw_bytes);

    // Stage 1: host byte mechanism plus materialization as a proper Lisp list.
    let byte_list_ns = timed_eval(
        "read-file-bytes",
        &format!("(def perf-bytes (read-file-bytes \"{path}\"))"),
        &mut session,
    );

    // Stage 2: the semantic pre-validation pass over the already materialized byte list.
    let validate_ns = timed_eval("utf8-all-bytes?", "(utf8-all-bytes? perf-bytes)", &mut session);

    // Stage 3: decode the known-valid list. Calling the internal worker intentionally
    // avoids counting the validation pass a second time; this is an observer probe,
    // not a replacement semantic path.
    let decode_ns = timed_eval(
        "utf8-decode-onto",
        "(def perf-decoded (utf8-decode-onto perf-bytes (quote ())))",
        &mut session,
    );

    // Stage 4: turn decoded Unicode scalar values into the runtime String.
    let materialize_ns = timed_eval(
        "unicode-scalars->string",
        "(unicode-scalars->string (car (cdr perf-decoded)))",
        &mut session,
    );

    // Control measurement: the public Lisp-owned path after the staged observations.
    let full_read_file_ns = timed_eval(
        "read-file",
        &format!("(read-file \"{path}\")"),
        &mut session,
    );

    let staged_total_ns = byte_list_ns + validate_ns + decode_ns + materialize_ns;
    let observed_sha = std::env::var("MY_LISP_OBSERVED_SHA").unwrap_or_else(|_| "unknown".into());

    println!(
        "((kind . cml-observed-my-lisp-utf8-stage-profile) \
         (upstream-channel . pr-317-head) \
         (my-lisp-sha . \"{observed_sha}\") \
         (file . \"{path}\") \
         (file-bytes . {file_bytes}) \
         (bootstrap-ns . {bootstrap_ns}) \
         (raw-os-read-ns . {raw_read_ns}) \
         (lisp-byte-list-boundary-ns . {byte_list_ns}) \
         (utf8-validate-ns . {validate_ns}) \
         (utf8-decode-worker-ns . {decode_ns}) \
         (scalar-to-string-ns . {materialize_ns}) \
         (staged-total-ns . {staged_total_ns}) \
         (public-read-file-ns . {full_read_file_ns}))"
    );
}
