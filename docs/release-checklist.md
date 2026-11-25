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

- [x] Verify `squad`, `send`, `export`, `models`, and `doctor` from the release
  binary (Linux x64, `cargo build --release --locked -p squad-cli`; the archive
  and macOS rows above stay pending).
- [x] Verify text input, JSON output, exit codes, pre-confirmation, `--no-wait`,
  accepted cursors and interrupt cleanup. **Bus logging is not verifiable: see
  defect 1.** One-shot recovery of killed contests is covered by
  `squad-core/tests/recovery.rs` and `m6_debate_recovery.rs`; the release test only adds the
  SIGINT path (exit 130, lock released, committed state exports).
- [x] Confirm models and doctor do not open rooms; room-open commands respect
  same-room locks; read-only export remains available while locked.
- [x] Verify room, debate, tournament, and grill markdown exports and unknown-cost
  doctor reporting using the release tests.
- [x] Verify census, crosscheck, survey, and probe commands, preflights,
  persistent variables, stop paths, and all export scopes from the release binary.
  Reports come from real runs against a loopback provider whose scripted answers
  cannot satisfy structured steps, so surveys/crosschecks/probes/tournaments end
  `FAILED` or degraded; full-content reports (supporting evidence, skips, plan
  versions) are exercised by `squad-core/tests/{census,crosscheck,survey,probe}_*`
  and by the seeded census report in the release test.
- [ ] Verify the four new-mode exports through RoomHandle, TUI, and offline CLI
  (RoomHandle and offline CLI done here; TUI rendering is covered by squad-tui
  snapshot tests, not re-run from the release binary).

Release-command evidence (Linux x64, 2026-11-25, branch `m7-release-tests`):

- `cargo test -p squad-cli --test release` builds `target/release/squad` once
  (`cargo build --release --locked -p squad-cli`) and runs every case against it
  with a wiremock OpenRouter fixture; no live calls.
- Covered: `--version` is `squad 0.1.0`; `--help` for send, export (all eight
  scopes), models and doctor with no side effects; `send` text and `--json`
  records, exit codes 0/1/2/130; refusal without `--yes` for census, crosscheck,
  survey, probe, debate and tournament (`<kind>.preflight` first, no ack, zero
  provider calls) and `--yes --no-wait` admission with an accepted cursor and no
  dispatch; every export scope from a real run while `main.lock` is held,
  including `-o` file output and scope-mismatch rejection; unknown-cost doctor
  line (`unknown-cost calls: 2; unreconciled charges (~)`); `/set CENSUS_PASSES`
  persistence overriding `SQUAD_CENSUS_PASSES` and appearing in the census
  preflight; SIGINT during a probe (exit 130, lock released, export works).

### Defects found by the release tests (reported, not fixed; product code is out of scope)

Each has an `#[ignore]`d test in `crates/squad-cli/tests/release.rs` that fails
until fixed (`cargo test -p squad-cli --test release -- --ignored`).

1. `--bus-log` / `--bus-log-bodies` do nothing for `send` (no consumer of
   `Cli::bus_log`; `crates/squad-cli/src/args.rs:22`), and `doctor --bus-log`
   exits with "Bus diagnostics: not implemented yet (milestone M3)"
   (`crates/squad-cli/src/main.rs:148`). Test:
   `bus_log_is_written_and_bodies_require_the_explicit_flag`.
2. A noninteractive refusal prints "Re-run with --yes in noninteractive mode."
   but never cancels the stored pending preflight
   (`crates/squad-cli/src/send.rs:224-232`, `return Ok(1)` before
   `ContestCancel`/`DebateCancel`). The re-run fails with `CONTEST_BUSY` /
   `DEBATE_BUSY` ("A census is already running...") until the plan expires.
   Repro: `SQUAD_DEBATE_CONFIRM_USD=0 squad send '@all $census plan' --json`
   (exit 1), then the same command with `--yes` (exit 1, CONTEST_BUSY).
   Reproduced for census, crosscheck, survey, probe, tournament and debate.
   Test: `rerun_with_yes_after_a_refused_preflight_is_admitted`.
3. (fixed) `squad-core` embeds 47 files from `crates/squad-core/prompts/`
   (`crates/squad-core/src/prompts/embedded.rs`, `include_str!("../../prompts/...")`),
   so `cargo package -p squad-core --list` ships all of them. Test:
   `packaged_squad_core_contains_every_embedded_prompt`.
4. Observation, not a defect: `tournament` with `--no-wait --yes` returns only an
   ack (no `tournament.preflight` record) because answer collection is
   question-first; with no `--yes` it prints the preflight and exits 1.

## Foundations setup verification (M7.P1.L2.T5)

- [ ] Run `./setup.sh` on macOS arm64 and Linux x64, with supported flags.
  Linux x64 done below; **macOS arm64 pending** (no Mac build host here).
- [x] Verify reruns, toolchain handling, missing-key guidance, `.env` preservation
  and permissions, doctor output, and the printed next command.

Setup evidence (Linux x64, Ubuntu, 2026-11-25). Run from a `git archive HEAD`
copy in a temp directory with a temp `HOME` (so the repository's `.env` was never
read or written), `CARGO_HOME`/`RUSTUP_HOME` pointing at the existing toolchain
and `CARGO_TARGET_DIR` inside the copy. Script kept outside the repo.

| Step | Command | Result |
| --- | --- | --- |
| Help | `./setup.sh --help` | exit 0, usage printed, no `.env` created |
| Bad flag | `./setup.sh --bogus` | exit 2, `Unknown option: --bogus` plus usage on stderr |
| First run | `--yes --runtime-only --no-build` | platform `Linux/x86_64; apt-get`, toolchain 1.99.1 with clippy and rustfmt, `Created .env with mode 0600.`, mode 600, content identical to `.env.example`, `OPENROUTER_API_KEY is required...` guidance, `Next command: ./target/release/squad`, exit 0 |
| Rerun | same | `Preserving existing .env.`, SHA-256 unchanged, mode 600, exit 0 |
| Loose mode | `.env` chmod 644 | `Warning: existing .env is readable by group or others; consider chmod 600 .env.`; mode left at 644 (never rewritten) |
| Key in `.env` | appended `OPENROUTER_API_KEY=sk-or-test-...` | `OPENROUTER_API_KEY is set.`; the key value was echoed 0 times |
| Key in process env | env var only, no `.env` key | `OPENROUTER_API_KEY is set.`; value not printed |
| Full run, no key | `--yes --runtime-only` | locked release build 1m05s, `squad doctor` ran: `Configuration: FAIL`, `Credential presence: missing`, data dir `0700`, `Overall: FAIL`, `squad doctor failed (exit 1)`, setup exit 1, next command still printed |
| Full rerun, key in env | same with key | `Overall: PASS`, `squad doctor succeeded (exit 0)`, setup exit 0, `Next command: ./target/release/squad` |

Not run on the real network: the development-tools path (`cargo install --locked
cargo-nextest cargo-insta`; neither tool is installed on this host) and the
privileged package-manager installs. Both are covered by the mocked
`scripts/test-setup.sh`, which `crates/squad-cli/tests/m6_setup.rs` runs
(including the Foundations `.env.example` equality check). The repository's own
`.env` was not touched or read.

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
- [x] Verify version 0.1.0, crate compatibility, changelog, LICENSE and NOTICE.
  SemVer policy, release notes, source distribution records and checksums stay
  pending (M7.P2.L1.T4).
- [x] Verify Apache-2.0 metadata for all six crates and compatible licences,
  original licence records, and NOTICE credits for every vendored asset/font.
- [x] Record the post-v1 roadmap including `$lattice`, `$delphi`, `$assemble`, and
  the grill/census-until-quiet meta-mode with explicit spending controls.

M6 evidence:

Documentation/version evidence:

- `squad --version` prints `squad 0.1.0`; `cargo metadata --no-deps` reports all
  six crates (`squad-protocol`, `squad-store`, `squad-agents`, `squad-core`,
  `squad-tui`, `squad-cli`) at 0.1.0, `license = "Apache-2.0"` (inherited from
  `[workspace.package]`), repository, rust-version, and every internal
  dependency pinned `^0.1.0` (`six_workspace_crates_carry_release_metadata_...`).
- `LICENSE` is the unmodified Apache License 2.0 text (header, terms, appendix);
  `NOTICE` carries the Squad copyright line and the FIGlet credits
  (`license_and_notice_records_are_complete`).
- Packaging: `cargo package -p <crate> --list --allow-dirty --offline` succeeds
  for all six crates, lists `Cargo.toml` and `src/`, and no `.env` or database
  (`every_crate_packages_without_secrets_...`). `cargo publish --dry-run -p
  squad-protocol` passes (it has no internal dependencies). **Dry-run publishing
  of squad-cli, squad-core and the other dependents fails until their
  dependencies exist on the registry** (`no matching package named squad-core
  found`), so the order must be protocol, store, agents, core, tui, cli, and only
  after defect 3 (prompts outside the squad-core package) is fixed. Nothing was
  published.
- Per-crate packages do not contain a copy of `LICENSE`/`NOTICE`; the SPDX
  `license` field satisfies crates.io, and the cargo-dist archive ships both
  files. Add per-crate copies if the owner wants them in registry tarballs.

Dependency and vendored-asset licence evidence:

Embedded and vendored assets (`git ls-files`, `include_str!`/`include_bytes!`
inventory, 2026-11-25). Only two third-party assets are vendored:

| Asset | Path | Origin | Original licence | Record and credit |
| --- | --- | --- | --- | --- |
| FIGlet font Standard (default) | `crates/squad-core/src/banner/fonts/standard.flf` (SHA-256 `37090ee2...b4e6`) | unmodified copy from figlet-rs 1.0.0 `resources/`; Glenn Chappell and Ian Chai 3/93, Latin extensions John Cowan, layout Paul Burton | BSD-3-Clause (cmatsuoka/figlet); the font header also permits modification if the modifier is named | `fonts/LICENSE-FIGlet.txt` (BSD-3-Clause text with the four copyright lines), `fonts/README.md`, the `.flf` header, and `NOTICE` |
| FIGlet font Small (compact) | `crates/squad-core/src/banner/fonts/small.flf` (SHA-256 `6326d8a7...adf`) | same distribution; Glenn Chappell 4/93, layout Paul Burton | BSD-3-Clause, same terms | same |

Both are parsed once from `include_str!`, never loaded from disk at runtime, and
are unmodified, so no modifier notice is needed. BSD-3-Clause is compatible with
Apache-2.0 distribution provided the notice is kept: it is in `NOTICE`, in the
font directory, and (through cargo-dist's archive) next to the binary.

First-party embedded content, no third-party licence: the 46 prompt templates and
`manifest.json` in `crates/squad-core/prompts/`, `crates/squad-store/migrations/*.sql`,
`crates/squad-cli/src/env-help.md`, `crates/squad-protocol/src/parser/*.json`
parser fixtures, and `.env.example`. Test snapshots (`*.snap`) and
`crates/squad-tui/docs/captures/` are generated by this repository's own tests.
There are no images, icon fonts, audio or other binary assets in the repository.
Syntax highlighting uses the `syntect` crate with its bundled grammars and themes
(a Cargo dependency whose licence terms travel with the crate, not a vendored
file). A `cargo-deny`/`cargo-about` dependency-licence audit has **not** been run
and remains pending.

Source distribution and release-note evidence:

Roadmap/shared-engine evidence:

- D71's build order (debate, grill, census, crosscheck, tournament, survey, probe)
  and the seven shipped modes are recorded in `CHANGELOG.md` under
  `[Unreleased]`; `$lattice`, `$delphi`, `$assemble`, and the grill/census
  until-quiet meta-mode (with its spending-control requirement) are listed under
  "Planned after v1" there. The v1 route is the human-directed `$grill`,
  `$census`, patch, `$crosscheck`, `$census` pipeline.

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
