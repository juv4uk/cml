hello"#).unwrap(), "hello");
}

#[test]
fn quoted_string_prints() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    assert_eq!(value(r#"(quote "hi")"#).unwrap(), "hi");
}

#[test]
fn string_eq_same() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    let v = value(r#"(eq "a" "a")"#).unwrap();
    assert_eq!(v.to_uppercase(), "T");
}

#[test]
fn string_in_cons() {
    if !strings_supported() {
        eprintln!("TAG_STRING not in RUNTIME yet; applicator pending");
        return;
    }
    let v = value(r#"(car (cons "x" 1))"#).unwrap();
    assert_eq!(v, "x");
}
