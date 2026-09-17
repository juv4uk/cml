use std::time::Instant;

use my_lisp::Session;

fn elapsed_ns(start: Instant) -> u128 {
    start.elapsed().as_nanos()
}

fn main() {
    my_lisp_host::install();
    let mut session = Session::default();

    let started = Instant::now();
    my_lisp::load_core_library(&mut session).expect("observed-current core bootstrap");
    let core_ns = elapsed_ns(started);

    let started = Instant::now();
    my_lisp::load_time_library(&mut session).expect("observed-current time bootstrap");
    let time_ns = elapsed_ns(started);

    let started = Instant::now();
    my_lisp::load_process_library(&mut session).expect("observed-current process/tcp bootstrap");
    let process_tcp_with_utf8_ns = elapsed_ns(started);

    let started = Instant::now();
    my_lisp::load_fs_library(&mut session).expect("observed-current fs bootstrap");
    let fs_with_repeated_utf8_ns = elapsed_ns(started);

    println!(
        "((kind . cml-observed-my-lisp-bootstrap-profile) \
         (core-ns . {core_ns}) \
         (time-ns . {time_ns}) \
         (process-tcp-with-utf8-ns . {process_tcp_with_utf8_ns}) \
         (fs-with-repeated-utf8-ns . {fs_with_repeated_utf8_ns}) \
         (measured-total-ns . {}))",
        core_ns + time_ns + process_tcp_with_utf8_ns + fs_with_repeated_utf8_ns
    );
}
