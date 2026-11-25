# Release checklist

Status: preparation only, version 0.1.0. No publication is authorized by this
checklist. The owner decides whether to publish, which artifacts to publish,
and when to push a version tag. Do not create tags, releases, taps, or crates.io
uploads during preparation.

Use the evidence fields below for command output, artifact hashes, platform,
commit, reviewer, and date. Empty fields are intentionally pending; scaffolding
does not prove M6 or M7 acceptance. See `docs/prd/milestones/M7-launch.md` and
`docs/prd/10-hardening-and-launch.md` for the owning requirements.

## Local artifact preparation (M7.P1.L2.T1–T3)

- [x] Generate with cargo-dist 0.34.0: `dist generate`; validate with `dist plan`.
- [x] Confirm release.yml triggers only on pushed version tags, with no branch,
  pull-request, or manual-dispatch trigger. The owner must approve publication
  before pushing a version tag: the generated workflow creates a GitHub Release.
- [x] Build Linux locally:
  `dist build --artifacts=local --target x86_64-unknown-linux-gnu`.
- [x] Inspect `target/distrib/squad-cli-x86_64-unknown-linux-gnu.tar.xz` for the
  `squad` executable, README, LICENSE, and NOTICE; verify its `.sha256` file.
- [ ] Build and inspect the equivalent `aarch64-apple-darwin` archive on macOS.
- [ ] Render `dist/homebrew/squad.rb` using `python3 dist/homebrew/render.py`
  after both archives are in `target/distrib`. The global release artifact job
  does this automatically through `[[dist.extra-artifacts]]`. It fills the
  version, tag, and SHA-256 placeholders and produces `target/homebrew/squad.rb`;
  cargo-dist stages it as `target/distrib/squad.rb`. It never pushes to a tap.
- [ ] Check the template and rendered formula with `ruby -c` when available.
- [x] Review CLI source inventory with `cargo package -p squad-cli --list`.
- [x] Run `cargo install --path crates/squad-cli --root <empty-temp-directory>`
  and check the installed `bin/squad --version` and `--help`.

Preparation evidence (Linux, 2026-11-24, based on source commit `7d671af`):

- `dist init --yes --with-json-config <local-config>` and `dist generate` completed
  with cargo-dist 0.34.0. `dist plan` selected only squad-cli, with macOS arm64
  on macos-15 and Linux x64 on ubuntu-22.04. No tag was created.
- Local Linux dist build passed. Archive contains `squad`, README, CHANGELOG,
  LICENSE, and NOTICE. SHA-256:
  `416013f8451cb152f69e32df52210099b153a5d81ac3c157be8d78a4944f82ff`.
- `cargo package -p squad-cli --list --allow-dirty` included CLI source, tests,
  manifest, lockfile, and README. A clean path install into a fresh temporary
  root passed; the installed executable reported `squad 0.1.0` and printed help.
- Workspace build, 125 tests (zero failures), Clippy with warnings denied, and
  formatting checks passed.
- Formula renderer fixture checks passed for both hashes, version, tag, archive
  names, and failure on missing archives. These were synthetic renderer inputs,
  not macOS installation evidence. Ruby was unavailable; syntax checks and actual
  Homebrew installs remain pending.
- A separate `cargo package --no-verify --allow-dirty -p squad-cli` probe failed:
  internal workspace path dependencies lack version requirements. Registry
  source packaging requires versioned internal dependencies and compatible
  published crate versions; that dependency work is outside this scaffold.

## Clean supported-platform installation (M7.P1.L2.T4–T8)

| Installation route | Platform | Artifact and SHA-256 | Command/output evidence | Date/reviewer |
| --- | --- | --- | --- | --- |
| Binary archive | macOS arm64 | | | |
| Binary archive | Linux x64 | | | |
| Rendered Homebrew formula | macOS arm64 | | | |
| Rendered Homebrew formula | Linux x64 | | | |
| Cargo source install | macOS arm64 | | | |
| Cargo source install | Linux x64 | | | |

- [ ] Verify `squad`, `send`, `export`, `models`, and `doctor` from clean artifacts.
- [ ] Verify text input, JSON output, exit codes, pre-confirmation, bus logging,
  `--no-wait`, accepted cursors, and one-shot recovery.
- [ ] Confirm models and doctor do not open rooms; room-open commands respect
  same-room locks; read-only export remains available while locked.
- [ ] Verify room, debate, tournament, and grill markdown exports and unknown-cost
  doctor reporting using the release tests.
- [ ] Verify census, crosscheck, survey, and probe commands, preflights, reports,
  persistent variables, stop paths, and all export scopes from clean artifacts.
- [ ] Verify the four new-mode exports through RoomHandle, TUI, and offline CLI,
  including supporting evidence, skips, plan versions, and honest stop caveats.

Release-command evidence:

## Foundations setup verification (M7.P1.L2.T5)

- [ ] Run `./setup.sh` on macOS arm64 and Linux x64, with supported flags.
- [ ] Verify reruns, toolchain handling, missing-key guidance, `.env` preservation
  and permissions, doctor output, and the printed next command.

Setup evidence:

## Documentation and records (M7.P1.L1; M7.P2.L1)

- [ ] M6 isolation, recovery, credential, terminal, cost, and performance gates pass.
- [ ] README commands, configuration/environment reference, personas, aliases,
  versions, persistent variables, costs, privacy, recovery, and examples match
  verified behavior. Include `doctor --env` and `CENSUS_PASSES` coverage.
- [ ] Document all seven contest modes: debate, tournament, grill, census,
  crosscheck, survey, and probe; cover question-first picks and the shared engine.
- [ ] Document D71's shared-engine build order and the human-directed
  `$grill` → `$census` → patch → `$crosscheck` → `$census` until quiet pipeline.
- [ ] Document new-mode blindness, costs/call counts, retry/skip behavior, reports,
  census estimate limits, crosscheck risk acceptance/bounds, survey uncertainty,
  probe unresolved explanations, and stop caveats.
- [ ] Verify version 0.1.0, SemVer policy, crate compatibility, changelog,
  release notes, source distribution records, checksums, LICENSE, and NOTICE.
- [ ] Verify Apache-2.0 metadata for all six crates and compatible licences,
  original licence records, and NOTICE credits for every vendored asset/font.
- [ ] Record the post-v1 roadmap including `$lattice`, `$delphi`, `$assemble`, and
  the grill/census-until-quiet meta-mode with explicit spending controls.

M6 evidence:

Documentation/version evidence:

Dependency and vendored-asset licence evidence:

Source distribution and release-note evidence:

Roadmap/shared-engine evidence:

## Demonstrations (M7.P2.L2.T1–T3, T7)

- [ ] Record the canonical timed question-first terminal demonstration in
  `docs/demo.md`: participants/spend, independent answers, picks, round/call/cost
  estimate, conditional confirmation, opponent turns, closing statements, and
  judge result. Link it from README.
- [ ] Label time cuts, retain the complete recording, and inspect for credentials.
- [ ] Record and link separate census, crosscheck, survey, and probe walkthroughs
  showing blind steps, costs, reports, stops, and exports. Include survey's five
  bands and probe's unexpected result/unresolved explanations.

Canonical demonstration and credential-inspection evidence:

New-mode walkthrough and credential-inspection evidence:

## Artifact inventory and owner decision (M7.P2.L1.T4; M7.P2.L2.T4–T6)

| Artifact | Version/commit | SHA-256 | Verification evidence | Approved for publication |
| --- | --- | --- | --- | --- |
| macOS arm64 archive | | | | |
| Linux x64 archive | | | | |
| Per-archive checksums and sha256.sum | | | | |
| Rendered Homebrew formula | | | | |
| Source archive/source package | | | | |
| Release notes | | | | |

Public repository visibility under D61 (URL/date/evidence):

Owner M7 boundary check-in:

Owner decision (publish/defer, exact artifacts, date):

Owner-approved version tag and commit:

Publication evidence (only after explicit authorization):

Post-publication installation verification:
