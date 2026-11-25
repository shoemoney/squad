# M7 — Launch

**Goal:** Prepare a documented, installable Squad release and reproducible terminal demonstration for the owner’s publication decision.

**Exit criteria:**

- M6 passes, including isolation, recovery, terminal safety, and credential checks.
- Clean installations pass for macOS arm64 and Linux x64 binaries, the Homebrew formula, and `cargo install`.
- `./setup.sh` passes supported-platform and rerun checks using the Foundations implementation.
- README commands, environment documentation, and examples match verified behavior.
- Apache License 2.0, `LICENSE`, `NOTICE`, crate SPDX metadata, compatible asset credits, changelog, checksums, and release artifacts are complete.
- The README links the recorded canonical demonstration, with edited waiting periods disclosed and no credentials exposed.
- The repository is public under D61; release artifacts are published only when the owner explicitly authorizes publication.
- Daily spending, all seven contest modes, question-first picks, and their export paths are documented and verified from release artifacts.
- New-mode walkthroughs, `CENSUS_PASSES`, blind-step boundaries, stop caveats, the recommended pipeline, and the expanded post-v1 roadmap are documented.

## Phase M7.P1 — Documentation and distribution

- **M7.P1.L1 — User documentation**
  - [ ] M7.P1.L1.T1 — Document `./setup.sh` as the quick start and explain binary, Homebrew, and `cargo install` options for `squad-cli` in `README.md`.
  - [ ] M7.P1.L1.T2 — Document configuration lookup, environment precedence, roster variables, personas, aliases, concrete versions, and stored `ROUNDS` and `GRILL_ROUNDS` for `squad-protocol`, `squad-agents`, and `squad-core` in `README.md`.
  - [ ] M7.P1.L1.T3 — Document addressing, quotes, commands, keyboard interaction, and the canonical question-first flow with the direct debate shortcut for `squad-cli` and `squad-tui` in `README.md`.
  - [ ] M7.P1.L1.T4 — Document estimates, optional judging, cases, daily spending, privacy, recovery, regeneration, export, storage, and `/clear` for `squad-core` in `README.md`.
  - [ ] M7.P1.L1.T5 — Document development checks, supported platforms, terminal requirements, troubleshooting, and all optional environment variables for the workspace crates in `README.md`.
  - [ ] M7.P1.L1.T6 — Document actual and pending spend, unknown costs, local-day rollover, machine-wide totals, `DAILY_MAX_REACHED`, in-flight completion, and Squad-only accounting for `squad-core` and `squad-tui` in `README.md`.
  - [ ] M7.P1.L1.T7 — Document tournament blind judging, private results, eliminations, trophies, grill shared-floor visibility, plan versions, passing, failure handling, call counts, and exports for `squad-core` and `squad-cli` in `README.md`.
  - [ ] M7.P1.L1.T8 — Document eligible picks, normal room use while pending, wrong-pick feedback, cancellation, restart persistence, and collection costs before preflight for `squad-core` and `squad-tui` in `README.md`.
  - [ ] M7.P1.L1.T9 — Document one-shot recovery behavior, models and doctor avoiding room open, `--no-wait`, and accepted cursors for `squad-cli` in `README.md`.
  - [ ] M7.P1.L1.T10 — Document census plan sources, assigned lenses, anonymous ratings, finder counts, estimate range and caveat, patches, quiet condition, discovery curve, singleton hint, and exports in `README.md`.
  - [ ] M7.P1.L1.T11 — Document crosscheck plan-blind inventory, cross-lens cells, no self-review, fresh audits, explicit human risk acceptance, bounded stop condition, optional extraction charge, and exports in `README.md`.
  - [ ] M7.P1.L1.T12 — Document survey collection and answer reuse, blind claims and votes, revotes, five report bands, uncertainty, settling questions, and exports in `README.md`.
  - [ ] M7.P1.L1.T13 — Document probe brief requirements, role split, sit-outs, blind proposals, committed predictions, decision tree, unexpected results, unresolved explanations, proposed checks, and exports in `README.md`.
  - [ ] M7.P1.L1.T14 — Document persistent `CENSUS_PASSES`, optional `SQUAD_CENSUS_PASSES`, new-mode call counts and skipped-member behavior, the shared engine, and the human-directed recommended pipeline in `README.md`.

- **M7.P1.L2 — Release artifacts**
  - [x] M7.P1.L2.T1 — Configure and verify `cargo-dist` builds for the `squad-cli` binary on macOS arm64 and Linux x64 in `dist-workspace.toml`.
  - [x] M7.P1.L2.T2 — Prepare and verify the Homebrew formula for `squad-cli` in `dist/homebrew/squad.rb`.
  - [x] M7.P1.L2.T3 — Verify package contents and clean `cargo install` for `squad-cli` using `crates/squad-cli/Cargo.toml` and its workspace dependencies.
  - [ ] M7.P1.L2.T4 — Verify all shipped commands, JSON output, exit codes, pre-confirmation, and markdown export from release artifacts in `crates/squad-cli/tests/release.rs`.
  - [ ] M7.P1.L2.T5 — Verify the Foundations `setup.sh` flags, reruns, `.env` preservation and permissions, doctor output, and next command for `squad-cli` in `docs/release-checklist.md`.
  - [ ] M7.P1.L2.T6 — Record supported-platform artifact and installation evidence for `squad-cli` in `docs/release-checklist.md`.
  - [ ] M7.P1.L2.T7 — Verify room, debate, tournament, and grill export scopes, offline export while the room is locked, `--no-wait`, and unknown-cost doctor reporting from `squad-cli` release artifacts in `crates/squad-cli/tests/release.rs`.
  - [ ] M7.P1.L2.T8 — Verify all four new-mode commands, preflights, reports, persistent variables, stop paths, and markdown export scopes, including locked-room offline export, from clean release artifacts in `crates/squad-cli/tests/release.rs`.

## Phase M7.P2 — Release preparation

- **M7.P2.L1 — Release records**
  - [ ] M7.P2.L1.T1 — Add Apache License 2.0 text and required notices for the six workspace crates in `LICENSE` and `NOTICE`, with `license = "Apache-2.0"` in their `Cargo.toml` files.
  - [ ] M7.P2.L1.T2 — Set workspace crate release metadata to `0.1.0` in `Cargo.toml` and write `CHANGELOG.md`.
  - [ ] M7.P2.L1.T3 — Verify reported versions and packaged crate compatibility in `crates/squad-cli/tests/release.rs`.
  - [ ] M7.P2.L1.T4 — Record binary checksums, formula and source distribution records, release notes, and the updated roadmap for `squad-cli` in `docs/release-checklist.md`.
  - [ ] M7.P2.L1.T5 — Verify Apache-2.0 compatibility, original licence records, and `NOTICE` credits for fonts embedded by `squad-core` and all other vendored workspace assets in `docs/release-checklist.md`.
  - [ ] M7.P2.L1.T6 — Record D71’s shared-engine build order and all seven v1 modes, and place `$lattice`, `$delphi`, `$assemble`, and the grill/census-until-quiet meta-mode on the post-v1 roadmap in `CHANGELOG.md` and `docs/release-checklist.md`.

- **M7.P2.L2 — Demonstration and publication decision**
  - [ ] M7.P2.L2.T1 — Write the timed canonical terminal demonstration and conditional-confirmation steps for `squad-cli` and `squad-tui` in `docs/demo.md`.
  - [ ] M7.P2.L2.T2 — Record the demonstration, label time cuts, retain the complete recording, and record credential inspection for `squad-cli` and `squad-tui` in `docs/demo.md`.
  - [ ] M7.P2.L2.T3 — Link the recorded demonstration of `squad-cli` and `squad-tui` from `README.md`.
  - [ ] M7.P2.L2.T4 — Prepare the release artifact inventory and owner publication decision record for `squad-cli` in `docs/release-checklist.md`.
  - [ ] M7.P2.L2.T5 — Record the public repository visibility under D61 and the owner’s M7 boundary check-in for the workspace crates in `docs/release-checklist.md`.
  - [ ] M7.P2.L2.T6 — If publication is explicitly authorized, publish only approved `squad-cli` artifacts and record installation verification in `docs/release-checklist.md`.
  - [ ] M7.P2.L2.T7 — Record and link separate census, crosscheck, survey, and probe walkthroughs showing blind steps, costs, reports, stop caveats, and exports; disclose time cuts and inspect for credentials in `docs/demo.md` and `README.md`.


