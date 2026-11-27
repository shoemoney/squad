# Trace — a brief you can audit

`$trace` turns a numbered source packet into a brief whose paragraphs point to
checked claims. Use it to compare source documents, summarize a research packet,
or separate what a set of records establishes from what it leaves uncertain.
It reads only the supplied packet and performs no browsing.

## Start

```text
@all $trace What retention obligations do these sources establish?
--
The service retains records for thirty days. Backups expire after seven days.
--
The backup policy applies only to production accounts.
```

Separate the question and each pasted source with a standalone `--` line.
Supply one to twelve sources. Attached quoted messages follow pasted segments.
Sources receive permanent numbers `S1`, `S2`, and so on in that order. The whole
room participates. The packet and all later bounded steps must fit every
assigned model's context; an oversized request returns `CONTEXT_TOO_LARGE`
before any contest call.

A headless invocation is:

```sh
squad send '@all $trace What is the retention period?
--
Records are retained for thirty days.' --yes
```

## What happens

1. **EXTRACT:** Every member independently extracts up to ten claims with short
   supporting quotes, counter-quotes and scope notes. There must be at least two
   accepted extractions.
2. **QUOTE CHECK:** Application code finds each contiguous quote in its cited
   source. It normalizes Unicode, curly quotes, dashes and whitespace, while
   preserving case. Quotes shorter than twelve normalized characters and stitched
   ellipses do not pass. Each accepted quote carries the original UTF-8 byte span.
   A missing quote becomes `quote_not_found`, is removed from subsequent prompts,
   and is never sent back for repair.
3. A blind judge merges overlapping claims into at most 25 canonical claims.
   Each one must reference extracted claims. The app attaches their verified
   quotes; the judge has no field for inventing a quotation. An empty claim set
   finishes immediately and releases the remaining reservation.
4. **VERIFY:** The app assigns two different reviewers per claim, balances their
   loads and persists the seeded assignment. Noncontributors fill both slots whenever
   two are available; a contributor fills a remaining slot only after distinct
   noncontributors are exhausted. Reviewers receive only their assigned claims and
   evidence, without contributor labels. They check entailment and scope; a
   missing qualifier requires a verbatim qualifier quote.
5. The app freezes the ledger. A claim is **unsupported** with no verified quotes
   or unanimous accepted `not_supported` verdicts; **unverified** with quotes but
   no verdicts; **supported** when every accepted verdict says `supported` and
   scope `ok`; otherwise it is **disputed**. A missing reviewer is never a vote.
6. **BRIEF:** The judge writes paragraphs citing supported claim IDs only.
   Disputed claims have a separate section. Unsupported and unverified claims
   remain separately visible, along with gaps and skipped members. The judge
   cannot change the ledger status.

Independent-source counts collapse declared `sharedOrigin` groups, including
transitively overlapping groups. Repeated quotations of one source still count
once. Contributing-model counts are shown separately: model agreement is not
independent source corroboration.

Invalid member structure or references get one diagnostic repair under D75.
Judge calls reserve up to three attempts, with the shared policy allowing one
validation repair. A second invalid response exhausts that repair. Transport
failures can consume the remaining attempt allowance. Failed reviewers leave
missing verdicts. Judge failure retains the committed evidence and ledger for
export. `/stop` fences pending work, and restart keeps accepted evidence,
provenance, prompt bundles and assignments.

## Cost

For `m` members, the normal upper bound is `2m + 2` calls and `4m + 6` reserved
attempts: six members mean 14 calls / 30 attempts; twelve mean 26 / 54.
Preflight prices frozen prompts, schemas, packet size and bounded prior outputs
against the resolved catalog. The reservation includes retries and reasoning;
actual cost depends on the roster, packet and generated output. Early completion
releases unstarted capacity. Unknown started charges remain reserved until
settled.

This scaffold's built-in roster has six seats (`config.rs::DEFAULT_MODELS`). A
custom twelve-seat roster requires twelve `SQUAD_MODEL_<NICK>` settings.

## Report and export

The report contains `[C1]`-style markers, claim statuses, support and counter-quotes
with source numbers and byte spans, independent-source and contributor counts,
verifier notes, qualifier quotes, flagged claims, missing quotes and skipped
members. Press F9 to select the next claim marker and inspect its quotes and source numbers.

```text
/export trace <contest-id>
```

```sh
squad export trace <contest-id> --output trace.md
```

Export adds the full numbered packet, every extraction, private-to-the-human
reader provenance, quote checks, canonical claims, verifier assignments and every
verdict. Frozen scheduling data, seeds and prompt bundles are excluded.

## Example run

The deterministic six-seat Wiremock workflow completes 14 provider calls and
reserves 30 attempts. Its report includes:

```text
# TRACE
Retention
Result: complete

Retention
Records persist thirty days. [C1]

[C1] Records retained thirty days
Status: Supported; independent sources: 1; contributing members: 2
Quote S1 bytes 0..37: > Records are retained for thirty days.
```

This is test evidence, not a paid live run. No live cost or wall time is claimed.

## Integration status

Enabled on `integration-v0.2.0`. The common outer dispatcher already routes all
new-mode work to its structured executor. The previously ignored full-room
regression now completes all fourteen provider calls through RoomHandle and is
un-ignored. The parser-checked Trace TIP is present. F9 cycles claim markers and
shows their source quotes and byte spans; the real App key path is covered.
The redundant Trace-only system-header rewrite is removed in favor of the
central blind judge manifest, preserving the complete mode system instruction.

## Validation

Validated on dev2 with `CARGO_BUILD_JOBS=4` at code head
`6943017e71b03789873f2687fee96cbfdf4b49e8`:

- `RUST_TEST_THREADS=4 cargo test --workspace --no-fail-fast`: 1,356 passing
  executions, zero failures, five ignores. One WAL subprocess repeats a passing
  test, so the unique passing-test count is 1,355. Four ignores are inherited;
  the fifth is the explicitly blocked Trace RoomHandle regression.
- All 30 active Trace checks pass: protocol 4, store 3, agents 3, workflows 7,
  tally/validation/sizing 9, export 1, view/discovery 3.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- The workspace release CLI suite builds and exercises the optimized binary.
- The documented parser example and help pass with temporary activation.
  The normal build remains disabled until integration, and no paid smoke was run.

Only the 24 permitted repository paths changed. Eleven of fourteen Trace
milestone tasks are checked off. `M8.P9.L4.T2`, `M8.P9.L5.T1` and
`M8.P9.L5.T3` retain explicit integration reasons and partial evidence.
