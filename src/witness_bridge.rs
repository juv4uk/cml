use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::x86_freestanding_metadata::X86CompiledProgram;

const MAX_COMPOSITE_DEPTH: usize = 256;
const MAX_COMPOSITE_CELLS: usize = 256;

#[derive(Debug)]
pub enum WitnessBridgeError {
    Io(String),
    Link(String),
    Execute(String),
    InvalidOutput(String),
    UnsupportedActual(wsm_os_target::Word),
    InvalidComposite(String),
}

impl fmt::Display for WitnessBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "witness bridge I/O error: {message}"),
            Self::Link(message) => write!(formatter, "witness bridge link error: {message}"),
            Self::Execute(message) => {
                write!(formatter, "witness bridge execution error: {message}")
            }
            Self::InvalidOutput(message) => {
                write!(
                    formatter,
                    "witness bridge invalid execution output: {message}"
                )
            }
            Self::UnsupportedActual(word) => {
                write!(
                    formatter,
                    "witness bridge cannot canonicalize target word {word:#x}"
                )
            }
            Self::InvalidComposite(message) => {
                write!(
                    formatter,
                    "witness bridge invalid composite actual: {message}"
                )
            }
        }
    }
}

impl std::error::Error for WitnessBridgeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArenaSymbols {
    begin: u64,
    next_ptr: u64,
    end_ptr: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ActualGraph {
    root: wsm_os_target::Word,
    cells: BTreeMap<wsm_os_target::Word, (wsm_os_target::Word, wsm_os_target::Word)>,
    sid8: BTreeMap<wsm_os_target::Word, u8>,
}

/// A semantics-blind recipe for constructing one target input value inside
/// the witness process. `Word` is already target-encoded; `Cons` only asks
/// the pinned runtime ABI to allocate a pair. No Lisp spelling or evaluation
/// exists in this transport layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X86InputValue {
    Word(wsm_os_target::Word),
    Sid8(u8),
    Cons(Box<X86InputValue>, Box<X86InputValue>),
}

impl X86InputValue {
    pub fn word(word: wsm_os_target::Word) -> Self {
        Self::Word(word)
    }

    pub fn sid8(bits: u8) -> Self {
        Self::Sid8(bits)
    }

    pub fn cons(car: X86InputValue, cdr: X86InputValue) -> Self {
        Self::Cons(Box::new(car), Box::new(cdr))
    }
}

pub fn canonical_actual_from_word(word: wsm_os_target::Word) -> Result<String, WitnessBridgeError> {
    let rendered = if word == wsm_os_target::NIL {
        "()".to_string()
    } else if word == wsm_os_target::CANONICAL_T {
        "t".to_string()
    } else if let Some(value) = wsm_os_target::decode_fixnum(word) {
        value.to_string()
    } else {
        return Err(WitnessBridgeError::UnsupportedActual(word));
    };

    Ok(format!("(value \"{rendered}\")"))
}

/// Execute an x86 witness and return only scalar actual values.
///
/// This compatibility API intentionally keeps the historical scalar surface:
/// composite values remain explicitly unsupported unless compiler-owned symbol
/// metadata is provided through execute_x86_actual_with_metadata.
pub fn execute_x86_actual(assembly: &str) -> Result<String, WitnessBridgeError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| WitnessBridgeError::Io(error.to_string()))?
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-witness-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&source, assembly).map_err(|error| WitnessBridgeError::Io(error.to_string()))?;
    fs::write(
        &launcher,
        "#include <stdint.h>\n#include <stdio.h>\nextern uint64_t wsm_entry(void *);\nint main(void) { printf(\"%llu\\n\", (unsigned long long)wsm_entry(0)); return 0; }\n",
    )
    .map_err(|error| WitnessBridgeError::Io(error.to_string()))?;

    let nucleus =
        crate::x86_freestanding::resolve_nucleus_asm_path().map_err(WitnessBridgeError::Link)?;
    let linked = Command::new("cc")
        .arg(&launcher)
        .arg(&source)
        .arg(&nucleus)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|error| WitnessBridgeError::Link(error.to_string()))?;

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&launcher);

    if !linked.status.success() {
        let _ = fs::remove_file(&executable);
        return Err(WitnessBridgeError::Link(
            String::from_utf8_lossy(&linked.stderr).into_owned(),
        ));
    }

    let output = Command::new(&executable)
        .output()
        .map_err(|error| WitnessBridgeError::Execute(error.to_string()))?;
    let _ = fs::remove_file(&executable);

    if !output.status.success() {
        return Err(WitnessBridgeError::Execute(format!(
            "exit status {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| WitnessBridgeError::InvalidOutput(error.to_string()))?;
    let word = stdout
        .trim()
        .parse::<wsm_os_target::Word>()
        .map_err(|error| WitnessBridgeError::InvalidOutput(error.to_string()))?;

    canonical_actual_from_word(word)
}

/// Execute an x86 witness and canonicalize proper/dotted composites using
/// compiler-owned metadata. Runtime memory is captured inside the child
/// process; Rust never dereferences a child-process pointer.
pub fn execute_x86_actual_with_metadata(
    compiled: &X86CompiledProgram,
) -> Result<String, WitnessBridgeError> {
    if !compiled.validate_symbol_metadata() {
        return Err(WitnessBridgeError::InvalidComposite(
            "compiler-owned symbol metadata failed validation".to_string(),
        ));
    }

    let capture = execute_x86_graph(&compiled.assembly)?;
    let rendered = render_actual(&capture, compiled)?;
    Ok(format!("(value \"{rendered}\")"))
}

/// Link one compiled input-entry artifact once, then invoke that exact
/// executable once per low-level input graph. Each invocation is a fresh
/// process (therefore a fresh bounded arena), but no recompilation or relink
/// occurs between inputs.
///
/// Host work is restricted to target-value transport: raw words and `wsm_cons`
/// allocation. The native Lisp artifact alone computes the output.
pub fn execute_x86_actuals_with_metadata_and_inputs(
    compiled: &X86CompiledProgram,
    inputs: &[X86InputValue],
) -> Result<Vec<String>, WitnessBridgeError> {
    if inputs.is_empty() {
        return Err(WitnessBridgeError::InvalidComposite(
            "input graph list must not be empty".to_string(),
        ));
    }
    if !compiled.validate_symbol_metadata() {
        return Err(WitnessBridgeError::InvalidComposite(
            "compiler-owned symbol metadata failed validation".to_string(),
        ));
    }

    let captures = execute_x86_graphs_with_inputs(&compiled.assembly, inputs)?;
    captures
        .iter()
        .map(|capture| {
            let rendered = render_actual(capture, compiled)?;
            Ok(format!("(value \"{rendered}\")"))
        })
        .collect()
}

fn execute_x86_graph(assembly: &str) -> Result<ActualGraph, WitnessBridgeError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| WitnessBridgeError::Io(error.to_string()))?
        .as_nanos();
    let base = std::env::temp_dir().join(format!("cml-witness-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&source, assembly).map_err(|error| WitnessBridgeError::Io(error.to_string()))?;
    fs::write(&launcher, graph_launcher_source())
        .map_err(|error| WitnessBridgeError::Io(error.to_string()))?;

    let nucleus =
        crate::x86_freestanding::resolve_nucleus_asm_path().map_err(WitnessBridgeError::Link)?;
    let linked = Command::new("cc")
        .arg("-no-pie")
        .arg(&launcher)
        .arg(&source)
        .arg(&nucleus)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|error| WitnessBridgeError::Link(error.to_string()))?;

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&launcher);

    if !linked.status.success() {
        let _ = fs::remove_file(&executable);
        return Err(WitnessBridgeError::Link(
            String::from_utf8_lossy(&linked.stderr).into_owned(),
        ));
    }

    let arena = resolve_arena_symbols(&executable)?;

    let output = Command::new(&executable)
        .arg(format!("0x{:x}", arena.begin))
        .arg(format!("0x{:x}", arena.next_ptr))
        .arg(format!("0x{:x}", arena.end_ptr))
        .output()
        .map_err(|error| WitnessBridgeError::Execute(error.to_string()))?;
    let _ = fs::remove_file(&executable);

    if !output.status.success() {
        return Err(WitnessBridgeError::Execute(format!(
            "exit status {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| WitnessBridgeError::InvalidOutput(error.to_string()))?;
    parse_graph_capture(&stdout)
}

fn execute_x86_graphs_with_inputs(
    assembly: &str,
    inputs: &[X86InputValue],
) -> Result<Vec<ActualGraph>, WitnessBridgeError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| WitnessBridgeError::Io(error.to_string()))?
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("cml-input-witness-{}-{nonce}", std::process::id()));
    let source = base.with_extension("s");
    let launcher = base.with_extension("c");
    let executable = base.with_extension("bin");

    fs::write(&source, assembly).map_err(|error| WitnessBridgeError::Io(error.to_string()))?;
    fs::write(&launcher, input_graph_launcher_source(inputs))
        .map_err(|error| WitnessBridgeError::Io(error.to_string()))?;

    let nucleus =
        crate::x86_freestanding::resolve_nucleus_asm_path().map_err(WitnessBridgeError::Link)?;
    let linked = Command::new("cc")
        .arg("-no-pie")
        .arg(&launcher)
        .arg(&source)
        .arg(&nucleus)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|error| WitnessBridgeError::Link(error.to_string()))?;

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&launcher);

    if !linked.status.success() {
        let _ = fs::remove_file(&executable);
        return Err(WitnessBridgeError::Link(
            String::from_utf8_lossy(&linked.stderr).into_owned(),
        ));
    }

    let arena = resolve_arena_symbols(&executable)?;
    let mut captures = Vec::with_capacity(inputs.len());
    for index in 0..inputs.len() {
        let output = Command::new(&executable)
            .arg(format!("0x{:x}", arena.begin))
            .arg(format!("0x{:x}", arena.next_ptr))
            .arg(format!("0x{:x}", arena.end_ptr))
            .arg(index.to_string())
            .output()
            .map_err(|error| WitnessBridgeError::Execute(error.to_string()))?;

        if !output.status.success() {
            let _ = fs::remove_file(&executable);
            return Err(WitnessBridgeError::Execute(format!(
                "input {index} exited with {}; stderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|error| WitnessBridgeError::InvalidOutput(error.to_string()))?;
        captures.push(parse_graph_capture(&stdout)?);
    }

    let _ = fs::remove_file(&executable);
    Ok(captures)
}

fn input_graph_launcher_source(inputs: &[X86InputValue]) -> String {
    fn emit_value(value: &X86InputValue, lines: &mut String, next_id: &mut usize) -> String {
        let id = *next_id;
        *next_id += 1;
        let name = format!("v{id}");
        match value {
            X86InputValue::Word(word) => {
                lines.push_str(&format!("    uint64_t {name} = 0x{word:016x}ULL;\n"));
            }
            X86InputValue::Sid8(bits) => {
                lines.push_str(&format!(
                    "    uint64_t {name} = wsm_sid8_new(0, {bits});\n"
                ));
            }
            X86InputValue::Cons(car, cdr) => {
                let car_name = emit_value(car, lines, next_id);
                let cdr_name = emit_value(cdr, lines, next_id);
                lines.push_str(&format!(
                    "    uint64_t {name} = wsm_cons(0, {car_name}, {cdr_name});\n"
                ));
            }
        }
        name
    }

    let mut builders = String::new();
    for (index, input) in inputs.iter().enumerate() {
        let mut lines = String::new();
        let mut next_id = 0;
        let root = emit_value(input, &mut lines, &mut next_id);
        builders.push_str(&format!(
            "static uint64_t build_input_{index}(void) {{\n{lines}    return {root};\n}}\n\n"
        ));
    }

    let mut cases = String::new();
    for index in 0..inputs.len() {
        cases.push_str(&format!(
            "        case {index}: input = build_input_{index}(); break;\n"
        ));
    }

    format!(
        r#"#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern uint64_t wsm_entry_with_input(void *, uint64_t);
extern uint64_t wsm_cons(void *, uint64_t, uint64_t);
extern uint64_t wsm_sid8_new(void *, uint64_t);
extern uint64_t wsm_sid8_bits(void *, uint64_t);

#define MAX_SEEN 256
#define CONS_ALIGNMENT 16
#define TAG_MASK 7
#define TAG_BOXED 7

static uint64_t seen[MAX_SEEN];
static size_t seen_len;

static int seen_before(uint64_t word) {{
    for (size_t i = 0; i < seen_len; ++i) {{
        if (seen[i] == word) return 1;
    }}
    return 0;
}}

static int dump_value(uint64_t word, uint64_t arena_begin, uint64_t arena_next,
                      uint64_t arena_end, size_t depth);

static int dump_value(uint64_t word, uint64_t arena_begin, uint64_t arena_next,
                      uint64_t arena_end, size_t depth);

static int dump_cons(uint64_t word, uint64_t arena_begin, uint64_t arena_next,
                     uint64_t arena_end, size_t depth) {{
    if (depth > MAX_SEEN) return -1;
    if (word == 0 || (word & TAG_MASK) != 0) return 0;
    if ((word % CONS_ALIGNMENT) != 0 || word < arena_begin ||
        word >= arena_next || word > arena_end - 16) return -1;
    if (seen_before(word)) return 0;
    if (seen_len == MAX_SEEN) return -1;
    seen[seen_len++] = word;

    uint64_t car = *(uint64_t *)(uintptr_t)word;
    uint64_t cdr = *(uint64_t *)(uintptr_t)(word + 8);
    printf("cell 0x%" PRIx64 " 0x%" PRIx64 " 0x%" PRIx64 "\n",
           word, car, cdr);
    if (dump_value(car, arena_begin, arena_next, arena_end, depth + 1) != 0) return -1;
    return dump_value(cdr, arena_begin, arena_next, arena_end, depth + 1);
}}

static int dump_value(uint64_t word, uint64_t arena_begin, uint64_t arena_next,
                      uint64_t arena_end, size_t depth) {{
    if ((word & TAG_MASK) == TAG_BOXED) {{
        uint64_t bits = wsm_sid8_bits(0, word);
        if (bits > 255) return -1;
        printf("sid8 0x%" PRIx64 " %" PRIu64 "\n", word, bits);
        return 0;
    }}
    return dump_cons(word, arena_begin, arena_next, arena_end, depth);
}}

{builders}
int main(int argc, char **argv) {{
    if (argc != 5) return 95;
    uint64_t arena_begin = strtoull(argv[1], NULL, 0);
    uint64_t arena_next_ptr = strtoull(argv[2], NULL, 0);
    uint64_t arena_end_ptr = strtoull(argv[3], NULL, 0);
    size_t input_index = (size_t)strtoull(argv[4], NULL, 10);
    uint64_t arena_end = *(uint64_t *)(uintptr_t)arena_end_ptr;

    uint64_t input = 0;
    switch (input_index) {{
{cases}        default: return 94;
    }}

    uint64_t root = wsm_entry_with_input(0, input);
    uint64_t arena_next = *(uint64_t *)(uintptr_t)arena_next_ptr;
    if (arena_next < arena_begin || arena_next > arena_end ||
        ((arena_next - arena_begin) % 16) != 0) return 95;

    printf("root 0x%" PRIx64 "\n", root);
    if (dump_value(root, arena_begin, arena_next, arena_end, 0) != 0) return 96;
    return 0;
}}
"#
    )
}

fn resolve_arena_symbols(executable: &std::path::Path) -> Result<ArenaSymbols, WitnessBridgeError> {
    let nm = Command::new("nm")
        .arg("-an")
        .arg(executable)
        .output()
        .map_err(|error| WitnessBridgeError::Link(format!("nm: {error}")))?;

    if !nm.status.success() {
        return Err(WitnessBridgeError::Link(format!(
            "nm failed: {}",
            String::from_utf8_lossy(&nm.stderr)
        )));
    }

    let mut begin = None;
    let mut next_ptr = None;
    let mut end_ptr = None;

    for line in String::from_utf8_lossy(&nm.stdout).lines() {
        let mut fields = line.split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        let Some(_) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else {
            continue;
        };

        let Ok(address) = u64::from_str_radix(address, 16) else {
            continue;
        };

        match name {
            "wsm_arena" => begin = Some(address),
            "wsm_arena_next" => next_ptr = Some(address),
            "wsm_arena_end" => end_ptr = Some(address),
            _ => {}
        }
    }

    match (begin, next_ptr, end_ptr) {
        (Some(begin), Some(next_ptr), Some(end_ptr)) => Ok(ArenaSymbols {
            begin,
            next_ptr,
            end_ptr,
        }),
        _ => Err(WitnessBridgeError::Link(
            "nm did not expose wsm-arena witness symbols".to_string(),
        )),
    }
}

fn parse_graph_capture(stdout: &str) -> Result<ActualGraph, WitnessBridgeError> {
    let mut graph = ActualGraph::default();
    let mut root_seen = false;

    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("root") => {
                if root_seen {
                    return Err(WitnessBridgeError::InvalidOutput(
                        "duplicate root record".to_string(),
                    ));
                }
                let Some(root) = fields.next() else {
                    return Err(WitnessBridgeError::InvalidOutput(
                        "root record has no word".to_string(),
                    ));
                };
                if fields.next().is_some() {
                    return Err(WitnessBridgeError::InvalidOutput(
                        "root record has extra fields".to_string(),
                    ));
                }
                graph.root = parse_word(root)?;
                root_seen = true;
            }
            Some("cell") => {
                let address = fields.next().ok_or_else(|| {
                    WitnessBridgeError::InvalidOutput("cell record missing address".to_string())
                })?;
                let car = fields.next().ok_or_else(|| {
                    WitnessBridgeError::InvalidOutput("cell record missing car".to_string())
                })?;
                let cdr = fields.next().ok_or_else(|| {
                    WitnessBridgeError::InvalidOutput("cell record missing cdr".to_string())
                })?;
                if fields.next().is_some() {
                    return Err(WitnessBridgeError::InvalidOutput(
                        "cell record has extra fields".to_string(),
                    ));
                }

                let address = parse_word(address)?;
                let car = parse_word(car)?;
                let cdr = parse_word(cdr)?;

                if graph.cells.insert(address, (car, cdr)).is_some() {
                    return Err(WitnessBridgeError::InvalidOutput(format!(
                        "duplicate cell address {address:#x}"
                    )));
                }
                if graph.cells.len() > MAX_COMPOSITE_CELLS {
                    return Err(WitnessBridgeError::InvalidComposite(format!(
                        "graph contains more than {MAX_COMPOSITE_CELLS} cells"
                    )));
                }
            }
            Some("sid8") => {
                let word = fields.next().ok_or_else(|| {
                    WitnessBridgeError::InvalidOutput("sid8 record missing word".to_string())
                })?;
                let bits = fields.next().ok_or_else(|| {
                    WitnessBridgeError::InvalidOutput("sid8 record missing bits".to_string())
                })?;
                if fields.next().is_some() {
                    return Err(WitnessBridgeError::InvalidOutput(
                        "sid8 record has extra fields".to_string(),
                    ));
                }

                let word = parse_word(word)?;
                let bits = bits.parse::<u16>().map_err(|error| {
                    WitnessBridgeError::InvalidOutput(format!(
                        "invalid sid8 payload {bits:?}: {error}"
                    ))
                })?;
                let bits = u8::try_from(bits).map_err(|_| {
                    WitnessBridgeError::InvalidOutput(format!(
                        "sid8 payload exceeds 8 bits: {bits}"
                    ))
                })?;
                if let Some(previous) = graph.sid8.insert(word, bits) {
                    if previous != bits {
                        return Err(WitnessBridgeError::InvalidOutput(format!(
                            "sid8 word {word:#x} changed payload from {previous} to {bits}"
                        )));
                    }
                }
            }
            Some("") | None => {}
            Some(other) => {
                return Err(WitnessBridgeError::InvalidOutput(format!(
                    "unknown graph record {other:?}"
                )));
            }
        }
    }

    if !root_seen {
        return Err(WitnessBridgeError::InvalidOutput(
            "graph capture omitted root".to_string(),
        ));
    }

    Ok(graph)
}

fn parse_word(value: &str) -> Result<wsm_os_target::Word, WitnessBridgeError> {
    let trimmed = value.trim();
    let trimmed = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    u64::from_str_radix(trimmed, 16).map_err(|error| {
        WitnessBridgeError::InvalidOutput(format!("invalid target word {value:?}: {error}"))
    })
}

fn render_actual(
    graph: &ActualGraph,
    compiled: &X86CompiledProgram,
) -> Result<String, WitnessBridgeError> {
    let mut active = BTreeSet::new();
    render_value(graph, graph.root, compiled, 0, &mut active)
}

fn render_value(
    graph: &ActualGraph,
    word: wsm_os_target::Word,
    compiled: &X86CompiledProgram,
    depth: usize,
    active: &mut BTreeSet<wsm_os_target::Word>,
) -> Result<String, WitnessBridgeError> {
    if depth > MAX_COMPOSITE_DEPTH {
        return Err(WitnessBridgeError::InvalidComposite(
            "composite depth exceeded".to_string(),
        ));
    }

    if word == wsm_os_target::NIL {
        return Ok("()".to_string());
    }
    if word == wsm_os_target::CANONICAL_T {
        return Ok("t".to_string());
    }
    if let Some(value) = wsm_os_target::decode_fixnum(word) {
        return Ok(value.to_string());
    }
    if let Some(_) = wsm_os_target::decode_symbol(word) {
        let name = compiled
            .symbol_name_for_word(word)
            .ok_or_else(|| {
                WitnessBridgeError::InvalidComposite(format!(
                    "symbol word {word:#x} has no compiler-owned metadata"
                ))
            })?
            .to_string();
        return Ok(name);
    }

    if wsm_os_target::tag(word) == wsm_os_target::Tag::Boxed as u64 {
        let bits = graph.sid8.get(&word).ok_or_else(|| {
            WitnessBridgeError::InvalidComposite(format!(
                "boxed word {word:#x} has no captured SID8 payload"
            ))
        })?;
        return Ok(format!("{bits:08b}"));
    }

    if wsm_os_target::tag(word) != wsm_os_target::Tag::Cons as u64 {
        return Err(WitnessBridgeError::UnsupportedActual(word));
    }

    render_cons(graph, word, compiled, depth, active)
}

fn render_cons(
    graph: &ActualGraph,
    start: wsm_os_target::Word,
    compiled: &X86CompiledProgram,
    depth: usize,
    active: &mut BTreeSet<wsm_os_target::Word>,
) -> Result<String, WitnessBridgeError> {
    let mut current = start;
    let mut chain = Vec::new();
    let mut items = Vec::new();

    loop {
        if depth + chain.len() > MAX_COMPOSITE_DEPTH {
            return Err(WitnessBridgeError::InvalidComposite(
                "composite depth exceeded".to_string(),
            ));
        }
        if !active.insert(current) {
            return Err(WitnessBridgeError::InvalidComposite(format!(
                "cycle detected at cons {current:#x}"
            )));
        }
        chain.push(current);

        let (car, cdr) = graph.cells.get(&current).ok_or_else(|| {
            WitnessBridgeError::InvalidComposite(format!(
                "cons pointer {current:#x} has no captured cell"
            ))
        })?;

        items.push(render_value(
            graph,
            *car,
            compiled,
            depth + chain.len(),
            active,
        )?);

        if *cdr == wsm_os_target::NIL {
            break;
        }

        if wsm_os_target::tag(*cdr) == wsm_os_target::Tag::Cons as u64 {
            current = *cdr;
            continue;
        }

        let tail = render_value(graph, *cdr, compiled, depth + chain.len(), active)?;
        for address in &chain {
            active.remove(address);
        }
        return Ok(format!("({} . {tail})", items.join(" ")));
    }

    for address in &chain {
        active.remove(address);
    }
    Ok(format!("({})", items.join(" ")))
}

fn graph_launcher_source() -> &'static str {
    r#"#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern uint64_t wsm_entry(void *);
extern uint64_t wsm_sid8_bits(void *, uint64_t);

#define MAX_SEEN 256
#define CONS_ALIGNMENT 16
#define TAG_MASK 7
#define TAG_BOXED 7

static uint64_t seen[MAX_SEEN];
static size_t seen_len;

static int seen_before(uint64_t word) {
    for (size_t i = 0; i < seen_len; ++i) {
        if (seen[i] == word) {
            return 1;
        }
    }
    return 0;
}

static int dump_cons(uint64_t word, uint64_t arena_begin, uint64_t arena_next,
                     uint64_t arena_end, size_t depth) {
    if (depth > MAX_SEEN) {
        fprintf(stderr, "graph depth exceeded\n");
        return -1;
    }
    if (word == 0 || (word & TAG_MASK) != 0) {
        return 0;
    }
    if ((word % CONS_ALIGNMENT) != 0 ||
        word < arena_begin ||
        word >= arena_next ||
        word > arena_end - 16) {
        fprintf(stderr, "invalid cons pointer: 0x%" PRIx64 "\n", word);
        return -1;
    }
    if (seen_before(word)) {
        return 0;
    }
    if (seen_len == MAX_SEEN) {
        fprintf(stderr, "too many captured cons cells\n");
        return -1;
    }

    seen[seen_len++] = word;

    uint64_t car = *(uint64_t *)(uintptr_t)word;
    uint64_t cdr = *(uint64_t *)(uintptr_t)(word + 8);
    printf("cell 0x%" PRIx64 " 0x%" PRIx64 " 0x%" PRIx64 "\n",
           word, car, cdr);

    if (dump_value(car, arena_begin, arena_next, arena_end, depth + 1) != 0) {
        return -1;
    }
    return dump_value(cdr, arena_begin, arena_next, arena_end, depth + 1);
}

static int dump_value(uint64_t word, uint64_t arena_begin, uint64_t arena_next,
                      uint64_t arena_end, size_t depth) {
    if ((word & TAG_MASK) == TAG_BOXED) {
        uint64_t bits = wsm_sid8_bits(0, word);
        if (bits > 255) return -1;
        printf("sid8 0x%" PRIx64 " %" PRIu64 "\n", word, bits);
        return 0;
    }
    return dump_cons(word, arena_begin, arena_next, arena_end, depth);
}

int main(int argc, char **argv) {
    if (argc != 4) {
        fprintf(stderr, "expected arena metadata arguments\n");
        return 95;
    }

    uint64_t arena_begin = strtoull(argv[1], NULL, 0);
    uint64_t arena_next_ptr = strtoull(argv[2], NULL, 0);
    uint64_t arena_end_ptr = strtoull(argv[3], NULL, 0);
    uint64_t arena_end = *(uint64_t *)(uintptr_t)arena_end_ptr;

    if (arena_begin == 0 || arena_end < arena_begin ||
        (arena_begin % CONS_ALIGNMENT) != 0 ||
        ((arena_end - arena_begin) % 16) != 0) {
        fprintf(stderr, "invalid arena metadata\n");
        return 95;
    }

    uint64_t root = wsm_entry(0);

    // wsm_entry owns the allocation side effects; snapshot the arena cursor
    // only after execution so every reachable cell is inside the captured
    // [arena_begin, arena_next) region.
    uint64_t arena_next = *(uint64_t *)(uintptr_t)arena_next_ptr;
    if (arena_next < arena_begin || arena_next > arena_end ||
        ((arena_next - arena_begin) % 16) != 0) {
        fprintf(stderr, "invalid post-execution arena cursor\n");
        return 95;
    }

    printf("root 0x%" PRIx64 "\n", root);

    if (dump_value(root, arena_begin, arena_next, arena_end, 0) != 0) {
        return 96;
    }
    return 0;
}"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Ir, Quoted};
    use crate::x86_freestanding::X86FreestandingBackend;

    fn quoted_symbol(name: &str) -> Quoted {
        Quoted::Sym {
            uppercased: name.to_ascii_uppercase(),
            original: name.to_string(),
        }
    }

    #[test]
    fn graph_decoder_canonicalizes_real_x86_proper_list() {
        let program = [Ir::Quote(Quoted::List(vec![
            quoted_symbol("A"),
            quoted_symbol("B"),
        ]))];
        let compiled = X86FreestandingBackend::new()
            .compile_program_with_metadata(&program)
            .expect("proper list must be admitted");

        let actual = execute_x86_actual_with_metadata(&compiled)
            .expect("composite x86 actual must be canonicalizable");

        assert_eq!(actual, "(value \"(A B)\")");
    }

    #[test]
    fn graph_decoder_canonicalizes_real_x86_dotted_list() {
        let program = [Ir::Quote(Quoted::DottedList(
            vec![quoted_symbol("A")],
            Box::new(quoted_symbol("B")),
        ))];
        let compiled = X86FreestandingBackend::new()
            .compile_program_with_metadata(&program)
            .expect("dotted list must be admitted");

        let actual = execute_x86_actual_with_metadata(&compiled)
            .expect("dotted composite x86 actual must be canonicalizable");

        assert_eq!(actual, "(value \"(A . B)\")");
    }

    #[test]
    fn graph_decoder_rejects_missing_cell() {
        let root = 0x1000;
        let graph = ActualGraph {
            root,
            cells: BTreeMap::new(),
            sid8: BTreeMap::new(),
        };
        let compiled = X86FreestandingBackend::new()
            .compile_program_with_metadata(&[Ir::Quote(quoted_symbol("A"))])
            .unwrap();

        let error = render_actual(&graph, &compiled).unwrap_err();
        assert!(matches!(error, WitnessBridgeError::InvalidComposite(_)));
    }

    #[test]
    fn graph_decoder_rejects_unknown_symbol() {
        let graph = ActualGraph {
            root: wsm_os_target::encode_symbol(999).unwrap(),
            cells: BTreeMap::new(),
            sid8: BTreeMap::new(),
        };
        let compiled = X86FreestandingBackend::new()
            .compile_program_with_metadata(&[Ir::Quote(quoted_symbol("A"))])
            .unwrap();

        let error = render_actual(&graph, &compiled).unwrap_err();
        assert!(matches!(error, WitnessBridgeError::InvalidComposite(_)));
    }

    #[test]
    fn graph_decoder_rejects_cycle() {
        let root = 0x1000;
        let mut cells = BTreeMap::new();
        cells.insert(root, (root, wsm_os_target::NIL));
        let graph = ActualGraph {
            root,
            cells,
            sid8: BTreeMap::new(),
        };

        let compiled = X86FreestandingBackend::new()
            .compile_program_with_metadata(&[Ir::Quote(quoted_symbol("A"))])
            .unwrap();

        let error = render_actual(&graph, &compiled).unwrap_err();
        assert!(matches!(error, WitnessBridgeError::InvalidComposite(_)));
    }

    #[test]
    fn graph_decoder_rejects_depth_overflow() {
        let backend = X86FreestandingBackend::new();
        let program = [Ir::Quote(quoted_symbol("A"))];
        let compiled = backend.compile_program_with_metadata(&program).unwrap();

        let mut cells = BTreeMap::new();
        for index in 0..=MAX_COMPOSITE_DEPTH {
            let address = ((index + 1) as u64) * 16;
            let cdr = if index == MAX_COMPOSITE_DEPTH {
                wsm_os_target::NIL
            } else {
                ((index + 2) as u64) * 16
            };
            cells.insert(address, (wsm_os_target::NIL, cdr));
        }

        let graph = ActualGraph {
            root: 16,
            cells,
            sid8: BTreeMap::new(),
        };
        let error = render_actual(&graph, &compiled).unwrap_err();
        assert!(matches!(error, WitnessBridgeError::InvalidComposite(_)));
    }

    #[test]
    fn scalar_api_still_rejects_composite_without_metadata() {
        let program = [Ir::Quote(Quoted::List(vec![quoted_symbol("A")]))];
        let compiled = X86FreestandingBackend::new()
            .compile_program_with_metadata(&program)
            .unwrap();

        let error = execute_x86_actual(&compiled.assembly).unwrap_err();
        assert!(matches!(error, WitnessBridgeError::UnsupportedActual(_)));
    }
}
