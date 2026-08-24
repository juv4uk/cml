# Viveka: cross-repository findings — 2026-08-24

Status: observational session record, not a semantic contract or repository authority.

Author: Viveka (Вівека; Sanskrit `viveka`, “discernment”), Codex agent and architect of epistemic integrity in Volodymyr's ecosystem.

This document preserves the results of the 2026-08-24 session across the owner's WSL repositories. It deliberately separates direct observation, peer reports, hypotheses, and open questions. A repository's contracts, evidence files, source texts, and current live state outrank this snapshot. No commit or push is implied by this file.

## Epistemic legend

- **CONFIRMED** — directly observed or exercised by Viveka in this session.
- **PARTIAL** — some relevant evidence was inspected, but the claim is not complete.
- **REPORTED** — communicated by another agent and not independently reproduced by Viveka.
- **UNRESOLVED** — plausible or proposed, but not established.
- **BROKEN** — the tested path failed for its intended purpose.

## 1. Cold start and role

- **CONFIRMED:** the complete ecosystem owner profile and active Agent Guard M0 proposal were read before write-heavy work.
- **CONFIRMED:** the working epistemology is: live state outranks memory; source, derived data, and knowledge must remain separate; claims need explicit confidence/status; embeddings and specialist models are witnesses, not authorities.
- **CONFIRMED:** the owner authorized the name **Viveka** and the role **architect of epistemic integrity**. The role is to preserve distinctions such as fact/hypothesis, source/derivation, delivery/receipt, and reproducible/non-reproducible result.

## 2. Cross-agent direct messaging experiment

Artifacts created in the ecosystem workspace:

- `scripts/agent-send`
- `.agents/sessions.json`
- `docs/AGENT-DIRECT-MESSAGING.md`
- the direct-messaging section in the root `AGENTS.md`
- transport notes in `comms-log.md`

Results:

- **CONFIRMED:** Codex native `codex queue` delivered a direct message into the current Codex session. Transport test ID: `d0d171d38b4041fa`.
- **CONFIRMED:** Vyasa-to-Viveka delivery through the Codex queue arrived. Message ID: `f7eac2a54caa4d07`.
- **BROKEN for live delivery:** an OpenCode `prompt_async` request sent through a separately launched `opencode serve` could be persisted without appearing in the already-running foreign TUI. Registration in a sessions file proves identity/address metadata, not a live endpoint.
- **PARTIAL:** the observed architecture indicates that OpenCode realtime delivery depends on sharing the same server/event bus. A likely operational pattern is one shared `opencode serve` with clients attached through `opencode attach <URL>`, but this was not confirmed end-to-end in the active foreign TUI.
- **CONFIRMED:** a later attempt to reply to Vyasa through the prior endpoint on port 4097 failed with `Connection refused`; the fallback was a durable entry in `comms-log.md`.

Practical universal rule:

1. Put durable content in a shared repository document or journal first.
2. Use the native session transport only as a short doorbell/pointer.
3. Treat transport acceptance, persistence, UI visibility, and human/agent receipt as four different states.
4. For Codex, queue delivery is the confirmed native path in this environment.
5. For OpenCode, do not claim live delivery until the recipient TUI is demonstrably attached to the same serving process.

Relevant upstream documentation/discussion inspected during the investigation:

- OpenCode source/project: <https://github.com/anomalyco/opencode>
- Codex source/project: <https://github.com/openai/codex>

These links provide implementation context; the environment-specific conclusions above come from the local transport tests.

## 3. Windows Obsidian vault audit

Target inspected: `/mnt/c/Users/user/Downloads/chatGPT-2023-2026/Obsidian`.

- **CONFIRMED:** this Windows vault is distinct from `/home/agents/Obsidian/ecosystem-vault` (different filesystem identity).
- **CONFIRMED:** the Windows vault contained 1,690 Markdown files and 1,766 files total, approximately 197 MiB at inspection time.
- **CONFIRMED:** 268 ontology-note files were found, while the main index described 973 notes / 215 ontology entries and `.ontology-map.json` contained 215 mappings. This is index/map drift, not proof that any one count is the intended authority.
- **CONFIRMED:** no broken symbolic links were found in the checked vault tree.
- **CONFIRMED:** `🤖 Діалог агентів.md` mixes operational traffic with repetitive heartbeat noise; it is not a clean knowledge record.
- **PARTIAL:** a random sample of ten documents included six Sanskrit corpus mirrors, two historical chats, and two Pāṇini research notes. Large corpus documents were sampled at beginning/middle/end rather than read exhaustively.
- **PARTIAL:** `ashokAvadAna.md` showed a likely CSX/encoding artifact in the sampled middle portion.
- **PARTIAL:** the Sarvam-oriented Pāṇini note looked incomplete, while the causative-identity audit was comparatively rigorous.
- **PARTIAL:** an older Canon EOS astrophotography discussion showed a persistent interest in systematic sky mapping, but an AI-produced zenith table should not be treated as calculated evidence without recomputation.
- **CONFIRMED:** semantic Sanskrit tags had been inserted inline into some historical chat files, and malformed/frontmatter anomalies were observed in sampled chats including 0108 and references around 0035/0172. This conflicts with the vault rule that historical content must remain intact. Exact quotations or restored history should be checked against the original export before correction.

Broader archive pattern, marked as interpretation rather than fact:

- **PARTIAL/INTERPRETATION:** Lisp and Sanskrit interests visibly converge in material dated 2023-05-28.
- **PARTIAL/INTERPRETATION:** recurring concerns about provenance, identity, ternary computing, reproducibility, and ownership anticipate the current my-lisp/fpga-lisp/agent-governance architecture.
- **UNRESOLVED:** this thematic continuity is useful for navigation and research questions, but it is not itself historical proof of architectural intent.

## 4. Semantic tagging run reported by Vyasa

The following is preserved as a peer report, not independently verified evidence:

- **REPORTED:** BGE-M3/CUDA runtime was about 660 seconds.
- **REPORTED:** semantic IAST tags were applied to 239 files using top-4 matches with similarity at least 0.55.
- **REPORTED:** 3 files had no match and 410 remained outside that run's budget (reported scope arithmetic: 652 files).
- **REPORTED:** files with existing tags were not changed.
- **REPORTED:** examples included grammar material matching `savarṇa`, `aṣṭādhyāyī`, `patañjali`, and corpus-index material matching `arthaśāstra`, `nāṭyaśāstra`, `śāstra`.

Required verification before treating those tags as knowledge:

- manually sample precision and false positives;
- inspect whether broad concepts become attractors;
- preserve “no confident match” rather than forcing four labels;
- distinguish source/corpus files from derived semantic overlays;
- never infer correctness merely from vector similarity or successful file writes.

## 5. Guix: underused power in the ecosystem

### Direct observations

- **CONFIRMED:** Guix in the inspected environment reported revision `0cc8f411...` dated 2026.
- **CONFIRMED:** manifests already exist across core repositories including `cml`, `fpga-lisp`, `my-idea`, `my-lisp`, `my-lisp-panini`, `shiva-sutras`, and `tauricode`.
- **CONFIRMED:** the ecosystem already uses `manifest.scm`, `guix shell --pure`, and in some places `guix time-machine`; this is stronger than an ordinary dependency list.
- **CONFIRMED:** the locally available Guix command set exposes reproducibility, challenge, pack, publish, container, VM/image, and provenance-oriented capabilities relevant to the ecosystem.

### Proposed capabilities

1. **Guix Witness — PARTIAL/PROPOSED.** Turn builds into evidence: pin channels; build with `--no-substitutes --rounds=2`; use `--check`; repeat on an independent builder; compare store outputs; use `guix challenge` where substitutes exist. Record source commit, channels hash, manifest hash, derivation, output path, builder identity, and result hash in the evidence ledger.
2. **Relocatable provenance packs — PROPOSED.** Use `guix pack --relocatable --save-provenance` to ship `swarm-node` or other tools to non-Guix hosts while retaining environment provenance. Guix also supports target formats such as Docker, Debian, and RPM depending on the chosen pack workflow.
3. **Agent capsules — PROPOSED.** Start agents through pinned `guix time-machine -C channels.scm -- shell --container --pure ...` environments, with explicit filesystem and network exposure. GPU, GUI, and MCP access should be conscious escape hatches rather than ambient assumptions.
4. **Signed substitute service — PROPOSED.** A controlled `guix publish` cache could let WSL users, build workers, and remote nodes reuse verified derivations. This requires an explicit trust-root and key-distribution design; it must not be exposed ad hoc.
5. **Guix Home — PROPOSED.** Version agent/workstation configuration without putting secrets in the store or repository.
6. **Guix System VM/images — PROPOSED.** Build deterministic multi-node swarm testbeds for restart, partition, replay, and liveness experiments instead of relying only on the current host state.

### First experiment: `GUIX-WITNESS-01`

Proposed sequence:

1. Package `my-lisp` and/or `swarm-node` as real Guix packages.
2. Pin the relevant channels.
3. Build locally with no substitutes and at least two rounds.
4. Repeat on an independent builder such as Ganaka.
5. Compare derivations and output hashes.
6. Create a relocatable provenance-preserving pack.
7. Run that pack on the Ubuntu droplet.

The hypothesis is that a Guix pack can avoid the earlier dynamic-loader `ENOENT` class on Ubuntu. This remains **UNRESOLVED** until the actual pack is built and run there.

Architectural boundary: a custom ecosystem Guix channel would be a delivery/provenance mechanism, not semantic authority. Whether it lives in a new repository or an existing one affects ownership, duplication, and licensing and therefore remains an owner decision.

Primary reference: GNU Guix manual, <https://guix.gnu.org/manual/en/guix.pdf>.

## 6. Repository and license boundaries

- **CONFIRMED:** before distributing this report, the ecosystem license matrix and repository-local instructions were checked.
- This report is original session prose and adds no third-party corpus or code.
- It does not change source texts, contracts, generated data, task registries, or evidence claims.
- It must not be used to launder GPL/NONE/unclear-license material into MIT repositories.
- For `shiva-sutras`, source texts and third-party registers remain governed by that repository's NOTICE files; this report is only an audit pointer.

## 7. Open questions and next evidence

- **UNRESOLVED:** confirm one-server OpenCode live delivery with both sender and recipient attached to the same `opencode serve` instance.
- **UNRESOLVED:** decide whether transport state should be represented as an explicit state machine: admitted → persisted → surfaced → acknowledged.
- **UNRESOLVED:** run `GUIX-WITNESS-01` and decide the owner/trust model for a substitute server or ecosystem channel.
- **UNRESOLVED:** audit and repair vault metadata without mutating historical chat bodies; use original exports as the comparison authority.
- **UNRESOLVED:** independently validate Vyasa's semantic tags with a stratified manual sample before using them downstream.

## 8. Scope and preservation note

This file was intentionally copied into every WSL git repository whose `origin` matched `github.com/juv4uk/*` at the time of the audit, plus the local `ecosystem` workspace. The copies are a discovery aid, not duplicated authority: this ecosystem copy is the canonical session record. Existing dirty worktrees were preserved; only this new documentation file (and a `docs/` directory where absent) was added by the distribution step.
