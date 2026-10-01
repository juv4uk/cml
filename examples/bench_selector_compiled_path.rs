//! #397 benchmark-only: compile a proven selector root+suffix path into direct CAR/CDR calls.
use cml::ir::{Ir, Quoted};
use cml::witness_bridge::execute_x86_actual;
use cml::x86_freestanding::X86FreestandingBackend;
use std::hint::black_box;

#[derive(Clone, Copy, Debug)]
enum Step {
    Car,
    Cdr,
}

fn sid(step: Step) -> sens::Sid8 {
    match step {
        Step::Car => sens::sid!(00000101),
        Step::Cdr => sens::sid!(00000110),
    }
}

fn selector_word(depth: usize, index: usize) -> String {
    let root = if index & 1 == 0 { "101" } else { "110" };
    let mut out = String::from(root);
    for bit in 0..depth {
        out.push(if (index >> (bit % usize::BITS as usize)) & 1 == 0 {
            '0'
        } else {
            '1'
        });
    }
    out
}

fn decode(word: &str) -> Result<Vec<Step>, String> {
    if word.len() < 3 {
        return Err("selector word shorter than root".into());
    }
    let root = match &word[..3] {
        "101" => Step::Car,
        "110" => Step::Cdr,
        other => return Err(format!("unsupported selector root {other}")),
    };
    let mut steps = vec![root];
    for bit in word[3..].bytes() {
        steps.push(match bit {
            b'0' => Step::Car,
            b'1' => Step::Cdr,
            _ => return Err("non-binary suffix".into()),
        });
    }
    Ok(steps)
}

fn selected_input(steps: &[Step]) -> Quoted {
    let mut current = Quoted::Int(42);
    for (i, step) in steps.iter().enumerate() {
        let dummy = Quoted::Int(1000 + i as i64);
        current = match step {
            Step::Car => Quoted::DottedList(vec![current], Box::new(dummy)),
            Step::Cdr => Quoted::DottedList(vec![dummy], Box::new(current)),
        };
    }
    current
}

fn direct_ir(steps: &[Step]) -> Ir {
    let mut expr = Ir::Quote(selected_input(steps));
    for step in steps.iter().rev().copied() {
        expr = Ir::App {
            func: Box::new(Ir::Sid(sid(step))),
            args: vec![expr],
        };
    }
    expr
}

fn compile_one(selector: &str) -> Result<String, String> {
    let steps = decode(selector)?;
    X86FreestandingBackend::new()
        .compile_program(&[direct_ir(&steps)])
        .map_err(|e| e.to_string())
}

fn verify(depth: usize) -> Result<(), String> {
    for index in 0..4usize {
        let selector = selector_word(depth, index);
        let steps = decode(&selector)?;
        let assembly = X86FreestandingBackend::new()
            .compile_program(&[direct_ir(&steps)])
            .map_err(|e| e.to_string())?;
        let actual = execute_x86_actual(&assembly).map_err(|e| e.to_string())?;
        if actual != "(value \"42\")" {
            return Err(format!("{selector}: expected 42, got {actual}"));
        }
        let calls =
            assembly.matches("call wsm_car").count() + assembly.matches("call wsm_cdr").count();
        if calls != steps.len() {
            return Err(format!(
                "{selector}: expected {} selector calls, got {calls}",
                steps.len()
            ));
        }
    }
    println!("VERIFY\tPASS\tdepth={depth}");
    Ok(())
}

fn measure(phase: &str, depth: usize, count: usize, pattern: &str) -> Result<(), String> {
    let mut checksum = 0usize;
    for i in 0..count {
        let index = if pattern == "repeated" { 0 } else { i };
        let selector = selector_word(depth, index);
        checksum ^= selector.len();
        if phase == "generate" {
            continue;
        }
        let steps = decode(&selector)?;
        checksum ^= steps.len();
        if phase == "decode" {
            continue;
        }
        let assembly = compile_one(&selector)?;
        checksum ^= assembly.len();
    }
    black_box(checksum);
    println!("CHECKSUM\t{checksum}");
    Ok(())
}

fn inspect(depth: usize, index: usize) -> Result<(), String> {
    let selector = selector_word(depth, index);
    let steps = decode(&selector)?;
    let assembly = compile_one(&selector)?;
    println!("SELECTOR\t{selector}");
    println!("STEPS\t{}", steps.len());
    println!("CAR_CALLS\t{}", assembly.matches("call wsm_car").count());
    println!("CDR_CALLS\t{}", assembly.matches("call wsm_cdr").count());
    println!("RUNTIME_PATH_OPS\t0");
    println!("ASSEMBLY_BYTES\t{}", assembly.len());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("verify") => verify(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4)),
        Some("inspect") => inspect(
            args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4),
            args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0),
        ),
        Some(phase @ ("generate" | "decode" | "compile")) => measure(
            phase,
            args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4),
            args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100),
            args.get(4).map(String::as_str).unwrap_or("repeated"),
        ),
        _ => Err(
            "usage: bench_selector_compiled_path verify DEPTH | inspect DEPTH INDEX | generate|decode|compile DEPTH COUNT repeated|random".into(),
        ),
    };
    if let Err(error) = result {
        eprintln!("ERROR: {error}");
        std::process::exit(2);
    }
}
