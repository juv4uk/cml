//! Lossless CML checkpoint for the bounded WSM FS record stream.

use std::io::{self, Read, Write};

fn main() {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .expect("read record stream");
    let stream = cml::fs::FsRecordStream::parse(&input).unwrap_or_else(|error| {
        eprintln!("cml-fs-stream: rejected record stream: {error:?}");
        std::process::exit(1);
    });
    let output = stream.to_bytes();
    if output != input {
        eprintln!("cml-fs-stream: record stream changed during round-trip");
        std::process::exit(1);
    }
    eprintln!(
        "cml-fs-ok records={} bytes={}",
        stream.records().len(),
        output.len()
    );
    io::stdout()
        .write_all(&output)
        .expect("write record stream");
}
