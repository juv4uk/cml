#!/usr/bin/env python3
# cml#398 research-only selector unroll threshold benchmark.
# Consumes the proven selector law: 0=CAR, 1=CDR.
# Compares straight-line unrolling vs a compact runtime suffix loop.

from __future__ import annotations

import argparse
import csv
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import tempfile

DEPTHS = (0, 1, 2, 4, 8, 16, 32, 64)


def checked(cmd: list[str], *, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, check=True)


def bits(depth: int) -> list[int]:
    return [((i * 5 + 3) ^ (i >> 1)) & 1 for i in range(depth)]


def c_array(depth: int) -> str:
    values = bits(depth)
    return ", ".join(map(str, values)) if values else "0"


def unrolled_body(depth: int) -> str:
    lines = ["    Node *p = root;"]
    for bit in bits(depth):
        lines.append(f"    p = p->{'cdr' if bit else 'car'};")
    lines.append("    return p;")
    return "\n".join(lines)


def generate_c() -> str:
    arrays = []
    funcs = []
    cases_bits = []
    cases_fn = []
    for d in DEPTHS:
        arrays.append(f"static const uint8_t BITS_{d}[{max(1,d)}] = {{{c_array(d)}}};")
        funcs.append(
            f"__attribute__((noinline)) static Node *unrolled_{d}(Node *root) {{\n"
            f"{unrolled_body(d)}\n}}\n"
        )
        cases_bits.append(f"        case {d}: *n = {d}; return BITS_{d};")
        cases_fn.append(f"        case {d}: return unrolled_{d};")

    return f'''#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Node {{
    struct Node *car;
    struct Node *cdr;
    uint64_t tag;
}} Node;

typedef Node *(*selector_fn)(Node *);

{os.linesep.join(arrays)}

__attribute__((noinline)) static Node *identity(Node *p) {{
    __asm__ volatile("" : "+r"(p));
    return p;
}}

__attribute__((noinline)) static Node *loop_select(
    Node *root, const uint8_t *path, size_t depth
) {{
    Node *p = root;
    for (size_t i = 0; i < depth; ++i) {{
        p = path[i] ? p->cdr : p->car;
    }}
    return p;
}}

{os.linesep.join(funcs)}

static const uint8_t *path_for_depth(size_t depth, size_t *n) {{
    switch (depth) {{
{os.linesep.join(cases_bits)}
        default: return NULL;
    }}
}}

static selector_fn unrolled_for_depth(size_t depth) {{
    switch (depth) {{
{os.linesep.join(cases_fn)}
        default: return NULL;
    }}
}}

static Node *build_path(size_t depth, const uint8_t *path, Node *nodes, Node *poison) {{
    for (size_t i = 0; i <= depth; ++i) {{
        nodes[i].car = NULL;
        nodes[i].cdr = NULL;
        nodes[i].tag = 0x1000u + (uint64_t)i;
    }}
    for (size_t i = 0; i < depth; ++i) {{
        poison[i].car = &poison[i];
        poison[i].cdr = &poison[i];
        poison[i].tag = 0xDEAD0000u + (uint64_t)i;
        if (path[i]) {{
            nodes[i].cdr = &nodes[i + 1];
            nodes[i].car = &poison[i];
        }} else {{
            nodes[i].car = &nodes[i + 1];
            nodes[i].cdr = &poison[i];
        }}
    }}
    return &nodes[0];
}}

int main(int argc, char **argv) {{
    if (argc != 4) {{
        fprintf(stderr, "usage: %s MODE DEPTH N\\n", argv[0]);
        return 2;
    }}
    const char *mode = argv[1];
    const size_t depth = (size_t)strtoul(argv[2], NULL, 10);
    const uint64_t reps = strtoull(argv[3], NULL, 10);
    size_t path_len = 0;
    const uint8_t *path = path_for_depth(depth, &path_len);
    selector_fn unrolled = unrolled_for_depth(depth);
    if (!path || !unrolled || path_len != depth || depth > 64) return 3;

    Node nodes[65];
    Node poison[64];
    Node *root = build_path(depth, path, nodes, poison);
    Node *expected = &nodes[depth];

    if (unrolled(root) != expected || loop_select(root, path, depth) != expected) {{
        fprintf(stderr, "selector parity failure at depth %zu\\n", depth);
        return 4;
    }}

    volatile uintptr_t sink = 0;
    for (uint64_t i = 0; i < reps; ++i) {{
        Node *out;
        if (strcmp(mode, "base") == 0) {{
            out = identity(root);
        }} else if (strcmp(mode, "unrolled") == 0) {{
            out = unrolled(root);
        }} else if (strcmp(mode, "loop") == 0) {{
            out = loop_select(root, path, depth);
        }} else {{
            return 5;
        }}
        sink ^= (uintptr_t)out;
    }}

    if (sink == 0x12345678u) printf("%zu\\n", depth);
    return 0;
}}
'''


def irefs(valgrind: str, binary: Path, mode: str, depth: int, n: int) -> int:
    proc = subprocess.run(
        [
            valgrind,
            "--tool=cachegrind",
            "--cache-sim=no",
            "--branch-sim=no",
            "--cachegrind-out-file=/dev/null",
            str(binary),
            mode,
            str(depth),
            str(n),
        ],
        text=True,
        capture_output=True,
        check=True,
    )
    match = re.search(r"I\s+refs:\s*([\d,]+)", proc.stderr)
    if not match:
        raise RuntimeError(f"Cachegrind I refs missing for {mode}/{depth}:\n{proc.stderr}")
    return int(match.group(1).replace(",", ""))


def symbol_sizes(nm: str, binary: Path) -> dict[str, int]:
    proc = checked([nm, "-S", "--size-sort", "--radix=d", str(binary)])
    sizes: dict[str, int] = {}
    for line in proc.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[1].isdigit():
            sizes[parts[-1]] = int(parts[1])
    return sizes


def version_line(cmd: list[str]) -> str:
    try:
        proc = checked(cmd)
        return (proc.stdout or proc.stderr).splitlines()[0].strip()
    except Exception:
        return "unknown"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", default="benchmarks/selector-unroll-threshold/results/current")
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--calls", type=int, default=100_000)
    args = ap.parse_args()

    gcc = shutil.which("gcc")
    valgrind = shutil.which("valgrind")
    nm = shutil.which("nm")
    if not gcc or not valgrind or not nm:
        raise SystemExit("gcc, valgrind and nm are required")

    repo = Path(__file__).resolve().parents[2]
    out_dir = (repo / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="cml-398-") as td:
        td_path = Path(td)
        src = td_path / "selector_bench.c"
        binary = td_path / "selector_bench"
        src.write_text(generate_c())
        checked([gcc, "-O2", "-std=gnu11", "-fno-omit-frame-pointer", "-o", str(binary), str(src)])

        for d in DEPTHS:
            checked([str(binary), "unrolled", str(d), "1"])
            checked([str(binary), "loop", str(d), "1"])

        sizes = symbol_sizes(nm, binary)
        git_sha = version_line(["git", "-C", str(repo), "rev-parse", "HEAD"])
        cpu = platform.processor() or version_line(["sh", "-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2-"])
        provenance = {
            "git_sha": git_sha,
            "cpu": cpu,
            "gcc": version_line([gcc, "--version"]),
            "valgrind": version_line([valgrind, "--version"]),
            "kernel": platform.release(),
            "calls": args.calls,
            "reps": args.reps,
            "note": "research mechanism benchmark; not production CML lowering; Cachegrind cache/branch simulation disabled",
        }
        (out_dir / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")

        raw_rows: list[dict[str, object]] = []
        summary_rows: list[dict[str, object]] = []
        for d in DEPTHS:
            by_mode: dict[str, list[int]] = {}
            for mode in ("base", "unrolled", "loop"):
                samples = []
                for rep in range(1, args.reps + 1):
                    value = irefs(valgrind, binary, mode, d, args.calls)
                    samples.append(value)
                    raw_rows.append({
                        "case_id": f"selector-depth-{d}",
                        "candidate": mode,
                        "family": "selector",
                        "semantic_depth": d,
                        "mode": "execute",
                        "rep": rep,
                        "i_refs": value,
                        "tree_steps": "" if mode == "base" else d,
                        "root_selections": "" if mode == "base" else 1,
                        "bits_consumed": "" if mode == "base" else d,
                        "generator_apps": "" if mode == "base" else d,
                        "registry_lookups": 0,
                        "residue_lookups": 0,
                        "cache_hits": 0,
                        "cache_misses": 0,
                        "allocations": 0,
                        "allocated_bytes": 0,
                        "object_bytes": d if mode == "loop" else 0,
                        "wire_bits": "",
                        "compiler_phase": "",
                        "machine_insts": "",
                        "code_bytes": sizes.get(f"unrolled_{d}", "") if mode == "unrolled" else sizes.get("loop_select", "") if mode == "loop" else sizes.get("identity", ""),
                        "loads": "",
                        "stores": "",
                        "branches": "",
                        "calls": "",
                        "spills": "",
                        "corpus_sha": "",
                        "binary_sha": "",
                        "git_sha": git_sha,
                        "guix_channels_sha": "",
                        "cpu": cpu,
                        "valgrind_version": provenance["valgrind"],
                    })
                by_mode[mode] = samples

            base = statistics.median(by_mode["base"])
            for candidate in ("unrolled", "loop"):
                full = statistics.median(by_mode[candidate])
                net = (full - base) / args.calls
                summary_rows.append({
                    "depth": d,
                    "candidate": candidate,
                    "median_total_i_refs": int(full),
                    "median_base_i_refs": int(base),
                    "net_i_refs_per_call": f"{net:.3f}",
                    "code_bytes": sizes.get(f"unrolled_{d}", 0) if candidate == "unrolled" else sizes.get("loop_select", 0),
                    "path_data_bytes": 0 if candidate == "unrolled" else d,
                    "packed_path_lower_bound_bytes": 0 if candidate == "unrolled" else (d + 7) // 8,
                    "static_footprint_bytes": (
                        sizes.get(f"unrolled_{d}", 0)
                        if candidate == "unrolled"
                        else sizes.get("loop_select", 0) + d
                    ),
                    "primitive_steps": d,
                    "runtime_path_decode_ops": 0 if candidate == "unrolled" else d,
                })

        raw_fields = list(raw_rows[0].keys())
        with (out_dir / "raw.tsv").open("w", newline="") as fh:
            writer = csv.DictWriter(fh, fieldnames=raw_fields, delimiter="\t")
            writer.writeheader()
            writer.writerows(raw_rows)

        with (out_dir / "summary.tsv").open("w", newline="") as fh:
            fields = list(summary_rows[0].keys())
            writer = csv.DictWriter(fh, fieldnames=fields, delimiter="\t")
            writer.writeheader()
            writer.writerows(summary_rows)

        print("depth\tcandidate\tnet_Irefs/call\tcode_bytes\tstatic_bytes\tprimitive_steps\truntime_path_decode")
        for row in summary_rows:
            print(
                f"{row['depth']}\t{row['candidate']}\t{row['net_i_refs_per_call']}\t"
                f"{row['code_bytes']}\t{row['static_footprint_bytes']}\t"
                f"{row['primitive_steps']}\t{row['runtime_path_decode_ops']}"
            )


if __name__ == "__main__":
    main()
