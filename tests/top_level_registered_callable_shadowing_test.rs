use cml::build::{Observation, compile_and_run};

#[test]
fn top_level_def_shadows_registry_callable_in_later_forms() {
    let source = r#"
        (def mod (lambda (a b) 123))
        (mod 17 5)
    "#;

    let observed = compile_and_run(source).expect("C backend build/run should complete");
    assert_eq!(observed, Observation::Value("123".to_string()));
}

#[test]
fn top_level_registered_callable_self_reference_uses_the_definition() {
    let source = r#"
        (def quotient
          (lambda (a b)
            (cond
              ((eq a 0) 0)
              (t (quotient (- a 1) b)))))
        (quotient 3 99)
    "#;

    let observed = compile_and_run(source).expect("recursive C backend build/run should complete");
    assert_eq!(observed, Observation::Value("0".to_string()));
}
