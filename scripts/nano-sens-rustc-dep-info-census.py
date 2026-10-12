#!/usr/bin/env python3
"""Audit actual Rust dep-info inputs after the Nano-SENS clean-run build.

This inventory is observational: it describes files listed on the RHS of the
compiler-generated Cargo .d dep-info files found in fresh CML/SENS target trees.
It is not a minimum seed closure proof and is not folded into the ELF size.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
from collections import defaultdict
from pathlib import Path


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git_blob_sha1(data: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def git_tree_index(repo: Path) -> dict[str, dict[str, object]]:
    output = subprocess.run(
        ["git", "-C", str(repo), "ls-tree", "-r", "-l", "HEAD"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    index: dict[str, dict[str, object]] = {}
    for row in output.splitlines():
        if "\t" not in row:
            continue
        left, rel = row.split("\t", 1)
        fields = left.split()
        if len(fields) < 4 or fields[1] != "blob":
            continue
        try:
            size = int(fields[3])
        except ValueError:
            continue
        index[rel] = {"git_blob_sha1": fields[2], "git_blob_bytes": size}
    return index


def normalize_display_path(
    path: Path,
    workspace: Path,
    sens_root: Path,
    wsm_root: Path,
    cargo_home: Path,
    target_roots: list[Path],
) -> tuple[str, str]:
    """Return a stable display path and preliminary source class."""
    resolved = path.resolve()
    # Check nested repositories before the CML root.
    for root, name in (
        (sens_root, "sens"),
        (wsm_root, "wsm-my-lisp"),
        (workspace, "cml"),
    ):
        try:
            rel = resolved.relative_to(root)
        except ValueError:
            continue
        if rel.parts and rel.parts[0] == ".git":
            return f"repo/{name}/.git/{'/'.join(rel.parts[1:])}", f"{name}_git_metadata"
        if "target" in rel.parts:
            if name == "sens":
                return f"generated/sens-target/{rel.as_posix().split('target/',1)[1]}", "generated_build_output"
            if name == "cml":
                return f"generated/cml-target/{rel.as_posix().split('target/',1)[1]}", "generated_build_output"
        return f"repo/{name}/{rel.as_posix()}", f"{name}_tracked_or_untracked"

    try:
        rel = resolved.relative_to(cargo_home)
        return f"external-cargo-home/{rel.as_posix()}", "external_cargo_home_source_or_metadata"

    for target_root in target_roots:
        try:
            rel = resolved.relative_to(target_root)
            label = "external-target"
            if "sens" in target_root.as_posix():
                label = "generated/sens-target"
            return f"{label}/{rel.as_posix()}", "generated_build_output"
        except ValueError:
            pass

    text = resolved.as_posix()
    if "/.rustup/" in text or "/rustlib/" in text or text.startswith("/rustc/"):
        return text, "external_rust_toolchain_or_sysroot"
    if text.startswith("/usr/") or text.startswith("/lib/") or text.startswith("/etc/"):
        return text, "external_host_input"
    return text, "other_external_input"


def resolve_dependency_token(
    token: str,
    depfile: Path,
    workspace: Path,
    sens_root: Path,
    wsm_root: Path,
    cargo_home: Path,
) -> Path | None:
    value = Path(token)
    if value.is_absolute():
        candidates = [value]
    else:
        # rustc dep-info can use paths relative to the Cargo package that emitted
        # the .d file. Prefer that package root before trying CML's root.
        if depfile.is_relative_to(sens_root):
            bases = [sens_root, workspace, wsm_root]
        elif depfile.is_relative_to(wsm_root):
            bases = [wsm_root, workspace, sens_root]
        else:
            bases = [workspace, sens_root, wsm_root]
        candidates = [base / value for base in bases]
        candidates.append(cargo_home / value)
    for candidate in candidates:
        try:
            resolved = candidate.resolve(strict=True)
        except (OSError, RuntimeError):
            continue
        if resolved.is_file():
            return resolved
    return None


def parse_depfile(
    depfile: Path,
    workspace: Path,
    sens_root: Path,
    wsm_root: Path,
    cargo_home: Path,
) -> tuple[list[Path], list[str], str | None]:
    raw = depfile.read_text(encoding="utf-8", errors="replace")
    # rustc emits make-style continuations. Workspace and Cargo paths in this
    # workflow contain no spaces, but shlex still decodes escaped tokens.
    raw = re.sub(r"\\\r?\n[ \t]*", " ", raw)
    body = "\n".join(line for line in raw.splitlines() if not line.lstrip().startswith("#"))
    if ":" not in body:
        return [], [], "no target separator ':' found"
    dependencies = body.split(":", 1)[1]
    try:
        tokens = shlex.split(dependencies, comments=False, posix=True)
    except ValueError as exc:
        return [], [], f"tokenization failed: {exc}"
    paths: list[Path] = []
    unresolved: list[str] = []
    seen: set[Path] = set()
    for token in tokens:
        if not token or token in ("|", "\\"):
            continue
        candidate = resolve_dependency_token(
            token, depfile, workspace, sens_root, wsm_root, cargo_home
        )
        if candidate is not None:
            if candidate not in seen:
                paths.append(candidate)
                seen.add(candidate)
        else:
            unresolved.append(token)
    return paths, unresolved, None

def main() -> int:
    if len(sys.argv) != 2:
        print("usage: nano-sens-rustc-dep-info-census.py OUTPUT_JSON", file=sys.stderr)
        return 2

    workspace = Path.cwd().resolve()
    sens_root = (workspace / "external/sens").resolve()
    wsm_root = (workspace / "wsm-my-lisp").resolve()
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()

    candidate_targets = [
        (workspace / "target").resolve(),
        (sens_root / "target").resolve(),
    ]
    configured_target = os.environ.get("CARGO_TARGET_DIR")
    if configured_target:
        candidate_targets.append(Path(configured_target).resolve())
    target_roots = list(dict.fromkeys(candidate_targets))

    depfiles: list[Path] = []
    for target in target_roots:
        if target.is_dir():
            depfiles.extend(p.resolve() for p in target.rglob("*.d") if p.is_file())
    depfiles = sorted(set(depfiles), key=lambda p: p.as_posix())
    if not depfiles:
        raise SystemExit("BLOCK: no Cargo/Rust dep-info .d files found under CML or SENS target trees")

    indexes = {
        "cml": git_tree_index(workspace),
        "sens": git_tree_index(sens_root),
        "wsm-my-lisp": git_tree_index(wsm_root),
    }
    roots = {
        "sens": (sens_root, indexes["sens"]),
        "wsm-my-lisp": (wsm_root, indexes["wsm-my-lisp"]),
        "cml": (workspace, indexes["cml"]),
    }

    file_inputs: dict[str, dict[str, object]] = {}
    depfile_entries: list[dict[str, object]] = []
    parse_errors: list[dict[str, str]] = []
    missing_tokens: set[str] = set()
    depfile_dependency_total = 0

    for depfile in depfiles:
        dependencies, unresolved, error = parse_depfile(
            depfile, workspace, sens_root, wsm_root, cargo_home
        )
        if error:
            parse_errors.append({
                "depfile": normalize_display_path(
                    depfile, workspace, sens_root, wsm_root, cargo_home, target_roots
                )[0],
                "error": error,
            })
            continue
        found = 0
        # Preserve unresolved path tokens separately. Normalize roots so the
        # report can match across isolated clean-run CARGO_HOME directories.
        for token in unresolved:
            candidate = Path(token)
            if not candidate.is_absolute():
                if depfile.is_relative_to(sens_root):
                    candidate = sens_root / candidate
                elif depfile.is_relative_to(wsm_root):
                    candidate = wsm_root / candidate
                else:
                    candidate = workspace / candidate
            display, _ = normalize_display_path(
                candidate, workspace, sens_root, wsm_root, cargo_home, target_roots
            )
            missing_tokens.add(display)
        depfile_dependency_total += len(dependencies) + len(unresolved)
        for dep in dependencies:
            display, preliminary_class = normalize_display_path(
                dep, workspace, sens_root, wsm_root, cargo_home, target_roots
            )
            if display in file_inputs:
                continue
            data = dep.read_bytes()
            item: dict[str, object] = {
                "path": display,
                "bytes_at_census": len(data),
                "content_sha256_at_census": sha256(data),
                "classification": preliminary_class,
            }
            matched_repo = None
            for repo_name, (repo_root, index) in roots.items():
                try:
                    rel = dep.relative_to(repo_root).as_posix()
                except ValueError:
                    continue
                # Ignore target outputs; all other paths under a checkout are
                # compared to the exact committed Git blob when one exists.
                if rel.startswith("target/"):
                    item["repository"] = repo_name
                    item["tracked_at_pinned_head"] = False
                    item["classification"] = "generated_build_output"
                    matched_repo = repo_name
                    break
                if rel.startswith(".git/"):
                    item["repository"] = repo_name
                    item["tracked_at_pinned_head"] = False
                    item["classification"] = f"{repo_name}_git_metadata"
                    matched_repo = repo_name
                    break
                pin = index.get(rel)
                item["repository"] = repo_name
                item["tracked_at_pinned_head"] = pin is not None
                if pin:
                    item.update(pin)
                    item["matches_pinned_git_blob"] = git_blob_sha1(data) == pin["git_blob_sha1"]
                    item["classification"] = f"{repo_name}_tracked_source_or_include"
                else:
                    item["classification"] = f"{repo_name}_untracked_or_generated_input"
                matched_repo = repo_name
                break
            file_inputs[display] = item
            found += 1
        depfile_entries.append({
            "path": next(
                (p.relative_to(workspace).as_posix() for p in [depfile] if p.is_relative_to(workspace)),
                depfile.as_posix()
            ),
            "bytes": depfile.stat().st_size,
            "resolved_regular_input_token_count": len(dependencies),
            "unresolved_token_count": len(unresolved),
            "unique_inputs_first_seen_in_this_depfile": found,
        })

    entries = sorted(file_inputs.values(), key=lambda item: str(item["path"]))
    by_class: dict[str, dict[str, int]] = defaultdict(lambda: {"files": 0, "bytes_at_census": 0})
    for item in entries:
        class_name = str(item["classification"])
        by_class[class_name]["files"] += 1
        by_class[class_name]["bytes_at_census"] += int(item["bytes_at_census"])

    report = {
        "schema": "nano-sens-rustc-dep-info-input-census/1",
        "status": "OBSERVED_CARGO_DEP_INFO_INPUTS_NOT_A_MINIMUM_SEED_CLOSURE_PROOF",
        "scope": "Unique existing regular-file paths listed on the RHS of rustc/Cargo .d dependency files found under the fresh CML and SENS target trees after the full lib-test runner ran. Includes compile-time source/include/build-script/proc-macro source paths emitted in those dep-info files; excludes outputs as seed bytes and does not claim all external host/toolchain dependencies are in .d files.",
        "pinned_revisions": {
            "cml": command("git", "rev-parse", "HEAD"),
            "sens": command("git", "-C", "external/sens", "rev-parse", "HEAD"),
            "wsm-my-lisp": command("git", "-C", "wsm-my-lisp", "rev-parse", "HEAD"),
        },
        "target_directories_scanned": [
            p.relative_to(workspace).as_posix() if p.is_relative_to(workspace) else str(p)
            for p in target_roots if p.is_dir()
        ],
        "dep_info_file_count": len(depfiles),
        "dep_info_parse_error_count": len(parse_errors),
        "dep_info_parse_errors": parse_errors,
        "dependency_tokens_resolved_across_depfiles_with_duplicates": depfile_dependency_total,
        "unique_existing_regular_input_files": len(entries),
        "unique_existing_regular_input_bytes_at_census": sum(int(e["bytes_at_census"]) for e in entries),
        "by_classification": dict(sorted(by_class.items())),
        "pinned_blob_mismatches": [
            {"path": e["path"], "expected_git_blob_sha1": e.get("git_blob_sha1"), "actual_sha256": e["content_sha256_at_census"]}
            for e in entries if e.get("tracked_at_pinned_head") and e.get("matches_pinned_git_blob") is False
        ],
        "unresolved_or_unreadable_dep_tokens": sorted(missing_tokens),
        "dep_info_files": depfile_entries,
        "limitations": [
            "Not a minimum closure proof: compiler dep-info is build-mode-, target-, and feature-specific.",
            "It does not enumerate all host tools, dynamic libraries, Cargo registry availability, Git refs/objects, or runtime-only file reads.",
            "Runtime-open paths remain in runtime-file-open-path-census.json; Cargo package graph remains in active Cargo graph files.",
            "Generated target outputs are inventory/evidence only and are not silently added to trusted-seed source bytes.",
            "Each tracked source/include is checked against the actual pinned Git blob; an empty mismatch list means worktree content matched the pin for all discovered tracked files."
        ],
    }
    output = Path(sys.argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({
        "schema": report["schema"],
        "status": report["status"],
        "dep_info_file_count": report["dep_info_file_count"],
        "unique_existing_regular_input_files": report["unique_existing_regular_input_files"],
        "unique_existing_regular_input_bytes_at_census": report["unique_existing_regular_input_bytes_at_census"],
        "dep_info_parse_error_count": report["dep_info_parse_error_count"],
        "pinned_blob_mismatch_count": len(report["pinned_blob_mismatches"]),
        "unresolved_or_unreadable_dep_token_count": len(report["unresolved_or_unreadable_dep_tokens"]),
        "by_classification": report["by_classification"],
    }, indent=2, sort_keys=True))
    # The audit is strict about parse errors and modified pinned inputs.
    if parse_errors:
        return 1
    if report["pinned_blob_mismatches"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
