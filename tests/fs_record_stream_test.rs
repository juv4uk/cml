use cml::fs::FsRecordStream;

// Captured from my-lisp `wsm-fs-record-producer` at commit 9c4650f.
// This fixture is evidence of the cross-repo bytes boundary, not a second
// implementation of WSM envelope semantics.
const PRODUCER_STREAM: &str = "((format . wsm-fs-root) (version 0 1) (revision . 1) (bindings (\"code\" . \"(lambda (x) x)\")) (objects \"(lambda (x) x)\"))\n((format . wsm-fs-object) (version 0 1) (address . \"(lambda (x) x)\") (value lambda (x) x))\n";

#[test]
fn accepts_pinned_my_lisp_canonical_stream_without_mutation() {
    let stream = FsRecordStream::parse(PRODUCER_STREAM.as_bytes()).expect("producer fixture");
    assert_eq!(stream.records().len(), 2);
    assert_eq!(stream.to_bytes(), PRODUCER_STREAM.as_bytes());
}
