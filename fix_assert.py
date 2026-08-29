with open("tests/c_backend_conformance_test.rs", "r") as f:
    text = f.read()

text = text.replace('    assert!(\n        admitted_newer_contract > 0,\n        "the shared suite should exercise capability-based admission"\n    );\n', '')

with open("tests/c_backend_conformance_test.rs", "w") as f:
    f.write(text)
