# Bass ackwards PRD

Start with the finished result. Squad agrees on observable acceptance tests, works backward to today's prerequisites, and saves a dependency-ordered PRD.

```text
@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts
@all $bass_ackwards_prd a production-ready status page -- We have a repository and a Linux server, but no deployed application.
```

From the CLI:

```sh
squad send '@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts'
```

1. Members independently propose three to eight acceptance tests. The judge groups equivalent tests without rewording them. Application code freezes tests supported by at least two distinct members, up to twelve.
2. Members independently build backward prerequisite trees, each bounded to twenty nodes and four levels. Every node serves a frozen acceptance test.
3. The judge merges those trees into milestones, phases and tasks. Every task cites a source node and an acceptance test. Dependencies point within the same or an earlier milestone.
4. Each member reviews through one assigned lens: missing steps, ways the plan can fail, or a concrete readback of one milestone. Comments must quote the delivered PRD. The application drops invented quotes without another call.
5. The judge accepts or rejects each surviving comment. The application protects permanent task identities and rejects unrelated edits.
6. Members propose up to three must-have additions each. Final assembly is add-only: existing text and order remain intact.
7. Application code checks task mapping, acceptance-test coverage, missing dependencies and cycles. It computes parallel levels and the longest dependency chain, then saves the Markdown PRD.

The report includes uncovered acceptance tests, skipped members, quote failures, review decisions, must-haves and provenance resolved to member names. A coverage finding does not invent extra work: the file is saved with findings clearly stated.

For an optional final human review, set `/set PRD_HUMAN_REVIEW 1` before starting. At `LAST LOOK`, `/answer <note>` adds a note and `/continue` proceeds to saving. Notes are limited to ten and 500 words together. Human notes are instructions; any impossible application is recorded as a conflict. With no notes there is no additional judge call. The default is off, so one command produces one file. A one-shot send with review enabled returns while waiting, before a file exists.

Six members use at most 28 calls and reserve 60 attempts; three use 16 calls and reserve 36 attempts. Human review reserves one extra judge call (three attempts). Empty review and addition rounds skip their judge calls. Actual dollars depend on selected models and text length; the normal estimate and worst-case reservation are shown in preflight. Unused reservations are released at termination.

The automatic destination is `<data_dir>/exports/prd-<title-slug>-<contestId>.md`, independent of the working directory. Files are written atomically with mode `0600`. The committed report precedes the write, allowing a restart during saving to reproduce the same bytes and destination. A later revision failure still saves the last good PRD. A merge failure saves nothing. If disk writing fails, the stored PRD remains available:

```text
/export bass_ackwards_prd
/export bass_ackwards_prd <contestId>
```

Export includes the final PRD and the complete accepted record, including dropped review comments. A successful CLI run prints `PRD saved to <path>` and emits `export.ready` in JSON output.

## Example run

Live smoke evidence will be recorded here after the shared judge dispatch prerequisite is resolved. No live spend has been incurred yet.

## Integration prerequisite

The scaffold's `crates/squad-core/src/runtime/judge.rs`, in `impl WorkExecutor for Executor`, sends only Probe/Census/Survey work directly to `self.crosscheck.execute`. New-mode judge work instead reaches `squad_agents::judge::JudgeExecutor`, whose old-kind allowlist rejects it before `Runner::execute` can reach the registered new-mode dispatch.

Required shared change: add `|| grant.work.kind.new_mode_kind().is_some()` to the initial `if` condition in that core adapter. This keeps all new-mode calls on their existing registered handlers and preserves grant ownership checks there. The mode worker has not edited that shared file. An end-to-end fake-provider run reproduces six completed done calls followed by a stuck `bass_ackwards_prd.freeze` work with no provider request.
