# Rank

Rank orders three to forty of your own items by a criterion you supply. Use it for feature priorities, bugs, risks, or candidates. Independent pair comparisons feed an application-computed ranking; a judge explains the top items and majority cycles.

**Availability:** enabled on `integration-v0.2.0`. Integrated verification and the live smoke are recorded below as they complete.

## Start a ranking

```text
@all $rank prioritize by customer impact relative to effort
--
Fix intermittent data loss
Make search tolerate spelling mistakes
Refresh the settings icons
```

Put the criterion before a standalone `--`, then one item per line. Supply 3–40 nonempty items, each at most 60 words. Select members with mentions, or use `@all` for the default roster. The judge is implicit; do not mention `@judge`.

## What happens

1. Preflight freezes the criterion, items, models, prompts, pair sample and per-member assignments. It shows estimated and reserved cost, asking for confirmation when required by your configured threshold.
2. Each member sees only its assigned pairs, with independently randomized A/B presentation. It chooses A or B for every pair and gives a reason of at most twelve words. It sees no other member's choices.
3. After the comparison barrier, the app fits Bradley–Terry strengths with a fixed phantom opponent. It computes order, win/loss records, gaps, majority cycles, split pairs and connected components. Missing batches contribute no votes. Fewer than two accepted batches or fewer than `n−1` judged pairs fails with `insufficient_comparisons`.
4. The blind judge explains the top five items (all for a shorter list), every displayed cycle, and caveats. It cannot change order or scores. If the judge fails, the fitted order remains available.
5. The report shows the numbered order, scores and records; clear/close gaps; cycle readings; split pairs; component warnings; design and per-member loads; and skipped members.

Up to seven items use every pair. Larger lists use a connected sample of `Q = min(n(n−1)/2, 3n)` pairs. Up to three distinct members judge each pair, with balanced loads and at most sixty judgments per member. The public command requires at least three members.

A clear gap means the fitted chance the higher item beats its neighbour is at least 70%; a close gap means less. These are fitted comparison probabilities, not confidence intervals. Cycles preserve conflicting majorities; the ten smallest combined vote margins display first. If missing batches disconnect the graph, order between components rests on no comparison.

## Cost and recovery

A run uses `m+1` normal calls. The default six-member roster makes seven calls and reserves fifteen attempts. Members have two attempts with one schema repair; the judge has three attempts and at most one schema repair. Preflight prices model-specific worst-case input and output, including repairs. Actual spend depends on tokens and prices; unused unstarted reservations release on completion, failure or stop.

`/stop` fences outstanding work and preserves accepted evidence. Recovery restores the exact pair design, A/B orders and accepted batches. It does not redraw or rerun accepted comparisons.

## Report and export

```text
/export rank
/export rank <contest-id>
```

Markdown export includes the report plus every accepted judgment with its member, A/B mapping, winner and reason, the win matrix, and unrounded fitted strengths and log scores. Prompt sources, private seeds, model bindings and persona maps are excluded.

## Example run

No integrated paid live result is claimed yet. The shared dispatch and member-repair defects are resolved on the integration branch. The credential-free direct-runtime wiremock workflow completes six batches and one explanation: nine planned judgments for three items, seven recorded attempts, and zero remaining reservation. Fixture results are not real-provider cost or quality measurements.

## Integration

The shared judge dispatcher routes Rank explanation work to its registered runner.
`MODE_INVALID_OUTPUT` uses the shared invalid-output path, authorizing one private
member repair. The mode switch is enabled; parser, help, completion and export
fixtures follow the enabled inventory. Judge compositions replace the original
identity header, including its literal `@fable`, with the common blind header.

Decision D102 caps the typed design's replication at the participant count. The
public parser retains the binding grammar's three-member minimum. The pure
boundary fixtures cover two-member designs for 3, 7, 8 and 40 items.

Public one-shot completion, private member repair, shared call-count and billing
checks are under integration verification. The default-six live smoke is pending.

## Validation record (2026-11-26, dev3)

With `CARGO_BUILD_JOBS=4`, the final full `cargo test --workspace --no-fail-fast` passed: 1,353 passing Cargo summary entries, zero failed, four ignored. The WAL subprocess repeats one passing case, so there are 1,352 unique passing tests. The 27 new owned tests cover protocol contracts (3), scoped store reads/repairs (3), agent output contracts (1), tally/design (7), provider/runtime workflows (7), report/export (3), and TUI projection (3). The existing release suite also built the optimized CLI successfully. Full-suite code head: `3989b50`; the subsequent split-pair presentation improvement is checked by the final owned suite.

All-target workspace Clippy with warnings denied and formatting checks pass. No real-provider smoke was run: enablement and the public failure-path acceptance remain blocked as described above. No provider cost, wall time, reserved amount or real report excerpt is claimed. Phase M8.P10 has twelve completed tasks; L5.T1 and L5.T3 remain open for integration, public enablement and live verification.

## Integration verification (2026-11-26)

The public OneShot RoomHandle tests complete and export a six-member ranking with seven calls and no remaining reservation. A first invalid member batch receives exactly one private repair; the resulting run completes in eight calls. All nine workflow tests and seven tally/design tests pass, including the two-member design boundary and deterministic fitted bits. The shared blind-header regression covers all twenty-four new-mode judge compositions. Full workspace gates and the paid integration smoke follow.
