#!/usr/bin/env python3
"""cml#397: compile selector root+suffix once, then measure direct native execution."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import platform
import re
import statistics
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXAMPLE = "bench_selector_compiled_path"
IREF_RE = re.compile(r"I\s+refs:\s+([0-9,]+)")
DEPTHS = (0, 1, 2, 4, 8, 16)
CANDIDATES = (
    ("nested-ir", "compile"),
    ("linear-recipe", "compile-linear"),
)


def sh(args, *, env=None, check=True):
    return subprocess.run(
        args, cwd=ROOT, env=env, check=check, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )


def output(args):
    try:
        return sh(args).stdout.strip()
    except Exception as exc:
        return f"unknown ({exc})"


def irefs_command(args, *, env=None):
    proc = sh(
        [
            "valgrind", "--tool=cachegrind", "--cache-sim=no",
            "--branch-sim=no", *map(str, args),
        ],
        env=env,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stdout + proc.stderr)
    match = IREF_RE.search(proc.stderr)
    if not match:
        raise RuntimeError("Cachegrind I refs missing\n" + proc.stderr)
    return int(match.group(1).replace(",", ""))


def text_size(obj):
    proc = sh(["size", "-A", str(obj)])
    for line in proc.stdout.splitlines():
        parts = line.split()
        if parts and parts[0] == ".text":
            return int(parts[1])
    raise RuntimeError(f".text size missing for {obj}")


def inst_count(obj):
    proc = sh(["objdump", "-d", str(obj)])
    return sum(
        bool(re.match(r"^\s*[0-9a-f]+:", line))
        for line in proc.stdout.splitlines()
    )


def compile_object(binary, depth, strategy, td):
    asm = td / f"d{depth}-{strategy}.s"
    obj = td / f"d{depth}-{strategy}.o"
    sh([str(binary), "emit", str(depth), "3", "1", strategy, str(asm)])
    sh(["cc", "-c", "-x", "assembler", str(asm), "-o", str(obj)])
    return text_size(obj), inst_count(obj)


def link_native(binary, depth, strategy, count, td, env):
    out = td / f"d{depth}-{strategy}"
    sh([
        str(binary), "link", str(depth), "3", str(count), strategy, str(out)
    ], env=env)
    return out


def parse_inspect(binary, depth):
    proc = sh([str(binary), "inspect", str(depth), "3"])
    parsed = {}
    for line in proc.stdout.splitlines():
        if "\t" in line:
            key, value = line.split("\t", 1)
            parsed[key] = value
    return parsed


def median_irefs(args, samples, *, env=None):
    return int(statistics.median(
        irefs_command(args, env=env) for _ in range(samples)
    ))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", required=True)
    ap.add_argument("--samples", type=int, default=3)
    ap.add_argument("--phase-count", type=int, default=1000)
    ap.add_argument("--native-count", type=int, default=32)
    ap.add_argument("--smoke", action="store_true")
    args = ap.parse_args()

    nucleus = os.environ.get("WSM_NUCLEUS_ASM")
    if not nucleus or not Path(nucleus).is_file():
        raise SystemExit("WSM_NUCLEUS_ASM must name the pinned nucleus.s")

    depths = (0, 4, 16) if args.smoke else DEPTHS
    samples = 1 if args.smoke else args.samples
    phase_count = min(args.phase_count, 200) if args.smoke else args.phase_count

    sh(["cargo", "build", "--quiet", "--release", "--example", EXAMPLE])
    binary = ROOT / "target/release/examples" / EXAMPLE
    env = os.environ.copy()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    rows = []

    with tempfile.TemporaryDirectory(prefix="cml397-") as td_name:
        td = Path(td_name)
        for depth in depths:
            sh([str(binary), "verify", str(depth)], env=env)
            parsed = parse_inspect(binary, depth)
            steps = int(parsed["STEPS"])

            structural = {}
            native = {}
            for strategy in ("baseline", "nested", "linear"):
                structural[strategy] = compile_object(binary, depth, strategy, td)
                exe = link_native(
                    binary, depth, strategy, args.native_count, td, env
                )
                native[strategy] = median_irefs([exe], samples)

            for pattern in ("repeated", "random"):
                common = {}
                for phase in ("generate", "decode"):
                    common[phase] = median_irefs(
                        [binary, phase, str(depth), str(phase_count), pattern],
                        samples,
                    )

                for candidate, compile_phase in CANDIDATES:
                    strategy = "nested" if candidate == "nested-ir" else "linear"
                    compile_irefs = median_irefs(
                        [
                            binary, compile_phase, str(depth),
                            str(phase_count), pattern,
                        ],
                        samples,
                    )
                    baseline_i = native["baseline"]
                    full_i = native[strategy]
                    delta_i = full_i - baseline_i
                    base_text, base_inst = structural["baseline"]
                    full_text, full_inst = structural[strategy]

                    rows.append({
                        "case_id": f"selector-d{depth}-{pattern}-{candidate}",
                        "candidate": candidate,
                        "family": "selector",
                        "semantic_depth": depth,
                        "mode": pattern,
                        "rep": "CML exact Sens8 CAR/CDR calls",
                        "path_steps": steps,
                        "runtime_path_ops": int(parsed["RUNTIME_PATH_OPS"]),
                        "car_calls": int(parsed["CAR_CALLS"]),
                        "cdr_calls": int(parsed["CDR_CALLS"]),
                        "generate_i_refs": common["generate"],
                        "decode_i_refs": common["decode"],
                        "compile_i_refs": compile_irefs,
                        "decode_delta_per_path":
                            (common["decode"] - common["generate"]) / phase_count,
                        "lower_emit_delta_per_path":
                            (compile_irefs - common["decode"]) / phase_count,
                        "native_batch": args.native_count,
                        "native_baseline_i_refs": baseline_i,
                        "native_full_i_refs": full_i,
                        "native_delta_i_refs": delta_i,
                        "native_i_refs_per_eval": delta_i / args.native_count,
                        "native_i_refs_per_step":
                            delta_i / args.native_count / steps,
                        "baseline_machine_insts": base_inst,
                        "full_machine_insts": full_inst,
                        "machine_inst_delta": full_inst - base_inst,
                        "machine_insts_per_step": (full_inst - base_inst) / steps,
                        "baseline_text_bytes": base_text,
                        "full_text_bytes": full_text,
                        "code_bytes_delta": full_text - base_text,
                        "code_bytes_per_step": (full_text - base_text) / steps,
                    })

    with (out / "results.tsv").open("w", encoding="utf-8", newline="") as f:
        writer = csv.DictWriter(
            f, fieldnames=list(rows[0]), delimiter="\t", lineterminator="\n"
        )
        writer.writeheader()
        writer.writerows(rows)

    environment = {
        "cml_git_sha": output(["git", "rev-parse", "HEAD"]),
        "sens_gitlink": output(["git", "ls-tree", "HEAD", "external/sens"]),
        "example_sha256": hashlib.sha256(
            (ROOT / "examples/bench_selector_compiled_path.rs").read_bytes()
        ).hexdigest(),
        "runner_sha256": hashlib.sha256(
            (ROOT / "benchmarks/selector-compiled/run.py").read_bytes()
        ).hexdigest(),
        "nucleus_sha256": hashlib.sha256(Path(nucleus).read_bytes()).hexdigest(),
        "rustc": output(["rustc", "--version"]),
        "valgrind": output(["valgrind", "--version"]),
        "cc": output(["cc", "--version"]).splitlines()[0],
        "python": platform.python_version(),
        "kernel": platform.release(),
        "cpu": next(
            (
                line.split(":", 1)[1].strip()
                for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")
            ),
            "unknown",
        ),
        "samples": samples,
        "phase_count": phase_count,
        "native_batch": args.native_count,
    }
    (out / "environment.json").write_text(
        json.dumps(environment, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    print(f"wrote {out / 'results.tsv'}")
    print(f"wrote {out / 'environment.json'}")


if __name__ == "__main__":
    main()
