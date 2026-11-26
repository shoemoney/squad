# Readback

Use `$readback` to find requirements that different readers would implement differently. It suits tickets, prompts, specifications and contract clauses. Members independently describe what they would build. A blind judge collects differences that change behaviour, and you choose the intended meanings before anything is rewritten.

```text
@all $readback The service retains records. The operator deletes backups.
@all $readback $READBACK_ROUNDS=1 The service retains records for thirty days.
@claude $readback
```

Paste the source after the command, quote a retained message, or mention a member to use that member's latest reply. The default loop has two rounds; `READBACK_ROUNDS` accepts one through four. The judge is implicit and cannot be mentioned as a participant.

Each round works as follows:

1. Every participant reads only the current spec in a fresh context and returns concrete behaviours, edge cases and assumptions.
2. The judge compares anonymous readings and produces a ledger of behavioural divergences anchored to verbatim passages. The human sees up to ten entries, lettered readings and member support counts.
3. Squad pauses at `READ IT BACK`. Participant holds are released so ordinary chat can continue. The pending questions and partial answers survive a restart and never expire.
4. Choose readings with `1b 2a`, or supply your own meaning with `/answer 3: retain records for thirty days`. Later answers replace earlier answers. Every entry answered closes the step; `/continue` closes it with the remaining entries open. No choice is preselected, and silence makes no choice.
5. The judge rewrites only resolved passages. The application verifies the exact change set, preserves every other character and checks the size bound. A later round reads the new spec independently.

An empty ledger ends with `converged`. Continuing without any answers ends with `no_readings_chosen` and makes no rewrite call. At the round limit the result is `rounds_complete`; the report explicitly says the last rewrite was not read back. Fewer than two accepted readings fails with `insufficient_readings`. Invalid member output gets one repair; exhausted members are skipped. Judge exhaustion keeps the last good spec. `/stop` preserves committed evidence and releases unused reservations.

With six members and two rounds there are at most 16 model calls and 36 reserved attempts. A member has two attempts and a judge three, including transport retries and at most one schema repair. The cost preview prices the frozen source, prompts, schemas and worst-case later evidence against the resolved models. Convergence releases unused rounds. Actual cost depends on model pricing, source length, repairs and how many rounds you choose to continue.

The report includes the final spec, per-round ledgers, reader attribution, chosen meanings, before/after pairs, open entries, skipped members and the application-computed convergence table. `/export readback [id]` also includes every spec version, interpretation and the human's raw answers. `squad send` returns at the human pause; use the same data directory for subsequent `/answer` or `/continue` sends.

## Example run

A paid example is pending the shared-runtime integration fixes listed below. No live cost, timing or provider result has been fabricated.

## Integration requirements

The mode worker owns only the readback paths. The following shared scaffold defects were reproduced with real `RoomHandle` plus wiremock, with no live provider calls:

- `crates/squad-core/src/runtime/judge.rs`, `Executor::execute`: new-mode judge work falls into `self.ordinary.execute`, whose `squad-agents/src/judge.rs::JudgeExecutor::execute` allowlist rejects it before the existing new-mode dispatch in `Runner::execute`. Route `grant.work.kind.new_mode_kind().is_some()` directly to `self.crosscheck.execute(context, grant, cancel)` alongside the existing structured-mode dispatch. This is required for all new-mode judge calls.
- `crates/squad-core/src/router/executor.rs`, `failed`: include `ErrorCode::MODE_INVALID_OUTPUT` in the `FailureClass::InvalidOutput` match. Currently member schema failures are treated as transport errors; supplying their required repair is consequently rejected. This prevents the D75 second attempt for every new-mode member.
- `crates/squad-protocol/src/parser/new_modes_tests.rs` and the disabled readback example in `parser/examples.json`: the original disabled-start fixture must be selected only while the mode is disabled, or explicitly force a disabled test context. Enabling a mode otherwise invalidates the scaffold fixture.
- `crates/squad-core/tests/human_step.rs`: the synthetic readback human-step fixture needs a `readback_runs` row with its ledger/setup before `/continue`; the implemented callback persists choices there. The old scaffold fixture created only a generic contest and step.

The end-to-end room tests remain active to expose the shared-runtime failures. The direct-runtime tests exercise the owned runtime, scoped readers, real wiremock transport, application validations, transactional barriers and export independently of the shared judge wrapper.

## Validation evidence

On dev3 with `CARGO_BUILD_JOBS=4`, the owned protocol, agents, store, tally, export and view suites pass. Direct-runtime wiremock workflows pass for partial choices and replacement, literal custom readings, unchanged open passages, convergence, no choices, reservation release, repaired non-verbatim ledgers, round limits, stop reports, and restart with a partial answer a year later. Room-level workflows are blocked by the two shared runtime defects above. Full workspace gates and the release smoke remain pending integration.
