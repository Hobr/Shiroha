# Roadmap Documentation Implementation Plan

## 1. Scope

Implement only the approved public roadmap documentation:

- create `ROADMAP.md` at repository root;
- update `README.md` with a concise link and aligned summary; and
- keep this task's `prd.md`, `design.md`, and `implement.md` as planning
  evidence.

Do not change Rust/WIT code, workspace version, dependencies, release metadata,
or implementation status. Do not create future Trellis child tasks.

## 2. Preconditions And Review Gate

- [x] Task-creation consent received.
- [x] Current repository and v0.1 executable evidence inspected.
- [x] Historical roadmap/session decisions checked and separated from current
      implementation status.
- [x] Product decisions captured in `prd.md`.
- [x] Complex-task `design.md` and `implement.md` created.
- [x] User reviews and approves all planning artifacts.
- [x] After approval, run the Phase 1.4 detail and activate the task.
- [x] In inline Phase 2, load `trellis-before-dev` before editing public files.

Planning approval authorizes the documentation change only. It does not
authorize implementation of any roadmap feature.

## 3. Phase A: Create The Public Roadmap

- [x] Add `ROADMAP.md` with a short purpose statement and capability-gated
      status legend.
- [x] Mark v0.1.0 `Completed` and list only current implementation evidence.
- [x] Add dependency-order overview and a compact release summary table.
- [x] Add full sections for v0.2.0 through v1.0.0 using one consistent shape:
      goal, user value, scope, non-goals, dependencies, validation point,
      release gates, and future task candidates.
- [x] Carry every approved contract from the PRD into the owning release:
      HSM, `redb`, REST, gRPC, service trust, Capability Profiles, stateless
      Nodes, at-least-once delivery, scheduling/aggregation, extensions,
      operations, packaging, and compatibility freeze.
- [x] Add a cross-cutting v1 production-readiness checklist.
- [x] Add an explicit post-v1 section for multi-Controller operation,
      Statecharts, additional databases/plugins/platforms, and advanced
      scheduling without implying commitment or timing.
- [x] State that roadmap task candidates are not active tasks and are created
      one at a time when a release begins.
- [ ] Translate README and `ROADMAP.md` headings, prose, tables, statuses, and
      release gates into Simplified Chinese; preserve literal code, commands,
      identifiers, protocol/crate names, and relative links. Include no calendar
      targets.

Rollback point: if the public document cannot describe a release without
inventing an undecided API or algorithm, stop and return to `prd.md`/`design.md`
instead of filling the gap with a placeholder promise.

## 4. Phase B: Integrate The README

- [x] Keep the existing v0.1 scope authoritative and unchanged except where a
      contradiction is discovered.
- [x] Add a visible link to `ROADMAP.md` in the Compatibility and Roadmap
      section.
- [x] Replace or retain the concise pre-v1 list only if it remains consistent
      with the detailed sequence; avoid duplicating all milestone content.
- [x] Ensure README still states same-release pre-v1 Host/Component
      compatibility and post-v1 multi-Controller deferral.

Rollback point: README must remain a current-use document. If roadmap detail
starts dominating it, move that material back to `ROADMAP.md` and keep only the
link and concise boundary.

## 5. Phase C: Content Verification

### 5.1 Evidence And Coverage

- [x] Verify the current version directly from `Cargo.toml`.
- [x] Verify v0.1 claims against current README, source, tests, and acceptance
      evidence.
- [x] Search deleted prototype terms and ensure none are marked implemented.
- [x] Confirm every item in `README.md:158-164` appears in one planned release.
- [x] Confirm every approved decision in `prd.md` appears in the public roadmap.
- [x] Confirm all versions v0.2.0 through v1.0.0 are present exactly once as
      milestone sections.
- [x] Confirm v0.2.0 is identified as the next release and no later version is
      marked started/completed.

Suggested audits:

```bash
rg -n '^## v0\.(1|2|3|4|5|6|7|8|9)\.0|^## v1\.0\.0' ROADMAP.md
rg -n 'Controller|Node|redb|REST|gRPC|Capability|HSM|aggregation|OpenTelemetry' ROADMAP.md
rg -n 'multi-Controller|Statechart|PostgreSQL|SQLite|Bash|NATS|Operator' ROADMAP.md
rg -n 'Q[1-4]|[0-9]+ (day|week|month|quarter)s?|target date|ETA' ROADMAP.md
```

The last command should return no scheduling commitments. Version numbers and
the historical v0.1 test evidence are not dates.

### 5.2 Cross-Layer Consistency

- [x] REST is public and gRPC is Controller-to-Node only.
- [x] Web/App owns user roles; Controller owns service/Node trust and framework
      validation only.
- [x] `redb` belongs to Controller, never Core.
- [x] Host remains authoritative for state and atomic transition commits.
- [x] Nodes remain stateless and cannot weaken resolved execution grants.
- [x] At-least-once and unknown-outcome language is identical across versions
      and production gates.
- [x] Aggregation and scheduler semantics do not imply exactly-once or optimal
      placement.
- [x] Public crates do not accidentally promise internal server/storage APIs.

### 5.3 Documentation And Repository Gates

Run from repository root:

```bash
git diff --check
nix develop -c just fmt
nix develop -c just check
```

Then rerun `git diff --check` and inspect the complete diff. `just fmt` is
mandatory before any eventual documentation commit under the project quality
spec. If a repository hook changes unrelated files, stop and separate the
unexpected change rather than folding it into this task.

Because public code is unchanged, the existing 41-test run is valid baseline
evidence. If README examples or commands change, rerun `nix develop -c just
test` and the affected example/validation commands before review.

## 6. Phase D: PRD Convergence And Review

- [x] Re-read `prd.md` top to bottom after the public draft exists.
- [x] Remove any duplicated or obsolete wording without losing requirement IDs,
      source anchors, decisions, or acceptance mappings.
- [x] Verify every acceptance criterion against the diff and record the
      evidence in the task notes or final report.
- [ ] Present `ROADMAP.md`, README diff, validation results, and any residual
      risk to the user for review.
- [x] Do not mark future version task candidates active or complete.

## 7. Change And Rollback Boundaries

Expected public change set:

```text
ROADMAP.md
README.md
```

Expected planning change set:

```text
.trellis/tasks/07-14-v0-2-to-v1-roadmap/prd.md
.trellis/tasks/07-14-v0-2-to-v1-roadmap/design.md
.trellis/tasks/07-14-v0-2-to-v1-roadmap/implement.md
.trellis/tasks/07-14-v0-2-to-v1-roadmap/task.json
```

No generated output, build artifact, version bump, dependency update, source
code, spec promotion, or future task directory belongs in this change.

If review rejects the release decomposition, edit planning artifacts first,
then regenerate the affected public roadmap sections. Do not patch the public
roadmap and leave its planning evidence contradictory.

## 8. Completion Definition

The documentation implementation is complete when:

1. all PRD acceptance criteria are satisfied;
2. `ROADMAP.md` and README agree;
3. formatting and repository checks pass;
4. no feature implementation or future task was created; and
5. the user approves the final public roadmap.

## 9. Spec Update Judgment

No `.trellis/spec/` update is required for this task. The public change defines
future release intent but implements no new API, storage schema, transport,
runtime behavior, or coding convention. Promoting the planned `redb`, REST,
gRPC, HSM, scheduler, or capability contracts into the active v0.1 code-specs
would incorrectly present future work as executable current behavior.

The `ntfs3` atomic-temporary-file stall encountered while activating the task
is a Trellis/tooling and host-filesystem issue, not a Shiroha runtime contract;
it does not belong in the backend code-spec library.
