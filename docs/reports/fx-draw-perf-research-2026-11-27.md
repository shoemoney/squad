# Full-effects draw performance research — 2026-11-27

Research only, on `perf-research-fx`, based on `456828e`. Measurements ran on dev3 on 2026-11-26 (America/Chicago); the requested report name uses 2026-11-27. No product fix or PR is included. Temporary instrumentation and experiments are retained as an unapplied patch.

## Finding

The composed gate is dominated by retained-history bookkeeping, not catalogue shaders. Cached Markdown and visible-row virtualization already exist. Two history-wide B-tree operations still run during draw: redundant state lookup for every retained card in composition preparation, and reinsertion of every retained message into the activity set. The latter also runs outside draw. Full effects keep frames scheduled during streams, exposing those costs, but disabling effects in the identical harness barely changes per-draw p95.

## Measurement scope and reproduction

Host: dev3, AMD Ryzen 9 8945HX, 32 logical CPUs, Linux 7.0.0-43-generic, Rust 1.99.1. Optimized bench with line-table debug information (`CARGO_PROFILE_BENCH_DEBUG=1`), default target CPU and allocator, no affinity, shared host. `CARGO_BUILD_JOBS=4`; one build or workload at a time except the explicitly discarded probe pair below. TestBackend, truecolor, 140x42, 30 seconds, twelve 40-delta/s producers, 10,000 complete history cards, typing and alternating scroll/follow. No network/provider traffic.

These are short research windows, not a replacement for 300 seconds per viewport on hueb. No Mac fix has been measured. `draw` includes the terminal call plus the harness's input assertions/layout check. Instrumented backend timings eagerly collect the lazy diff before writing; that adds a small Vec allocation and excludes physical terminal I/O. Stage percentages divide summed stage wall time by summed draw wall time; nested stages are not additive to parents. CPU sample percentages cover the whole process and must not be read as draw percentages.

## Baselines and experiments

| Run | Seconds | Draw p95 / p99 ms | Key p95 ms | CPU, % one core | End RSS KiB |
|---|---:|---:|---:|---:|---:|
| Uninstrumented Linux baseline | 30 | 3.120 / 4.674 | 18.142 | 15.298 | 255444 |
| Instrumented control | 30 | 3.106 / 5.332 | 18.478 | 15.595 | 255656 |
| Same renderer, FX off | 30 | 2.911 / 5.037 | 18.374 | 14.132 | 255820 |
| Read card state directly | 30 | 2.806 / 4.803 | 18.085 | 13.997 | 255932 |
| Activity fast-path prototype | 30 | 2.883 / 5.630 | 17.513 | 10.232 | 255684 |
| Unchanged history skipped | 30 | 1.895 / 2.509 | 17.706 | 11.165 | 255932 |
| History + activity prototypes | 30 | 1.495 / 1.851 | 16.402 | 4.866 | 255848 |
| Anchors omitted (upper bound only) | 30 | 2.482 / 5.366 | 18.278 | 14.997 | 250824 |

All 30-second windows delivered 1,801 frames and 14,412 deltas; typed-character assertions passed. Baseline startup was 1,271.7 ms; load started at 3.08/1.41/0.97 and ended 2.08/1.32/0.96. The sequential instrumented controls/probes mostly ran with one-minute load below 1. First instrumented control was 2.932 ms p95; the later control was 3.106 ms. Treat differences of a few hundredths of a millisecond as noise. One probe launch overlapped the preceding probe's tail by about two seconds; both logs are explicitly named `discarded-*-overlap.log`, excluded from all tables/claims, and were repeated sequentially.

The FX-off comparison is the **same composed renderer**, not `twelve_streams.rs`. That older bench says explicitly that it has no Squad widgets, Markdown or effects dependencies and uses a simplified cached ASCII fixture. Its supplied 0.3–0.5 ms result is not a controlled estimate of the effects tax. Here full/off was 2.932/2.911 ms p95: no meaningful per-draw improvement from disabling effects. The real scheduler may still use less CPU in Off mode by drawing less frequently; the bench forces 60 Hz in both modes.

### Comparison with prior Linux and supplied macOS results

The earlier [Linux report](../performance.md#real-widget-twelve-stream-full-effects-gate) contains three **300-second** runs at each viewport:

| Viewport | Earlier Linux p95 min / median / max ms | Earlier p99 range ms |
|---|---:|---:|
| 140x42 | 3.656 / 3.798 / 3.947 | 4.540–4.734 |
| 80x24 | 3.408 / 3.420 / 3.671 | 4.254–4.428 |
| 200x60 | 4.043 / 4.124 / 4.132 | 4.768–4.992 |

The supplied hueb/macOS 27 run 45912463641 reports p95 10.8–11.6 ms, p99 12.1–13.2 ms, key p95 34–40 ms, CPU about 44%, RSS about 240–287 MB and 15–16k frames per 300 seconds. Compared with the older Linux medians, that p95 range is **2.62–3.39×**, without an exact supplied per-viewport mapping. For the current short 140x42 baseline, it is **3.46–3.72×**. At 140x42 the older Linux median gives 2.84–3.05×. Thus 2× is an optimistic planning assumption, not the observed ratio. Short duration, different revisions/profiles and shared-host conditions prevent treating these as a controlled hardware benchmark or a proven regression; the current CPU is higher than the older 9.4–10.8% at 140x42 even though its p95 is lower.

## Where frame time goes

The later 30-second control averaged **1.907 ms per draw**. Disjoint top-level stages:

| Stage | Mean ms/frame | Share of draw | Stage p95 ms |
|---|---:|---:|---:|
| Layout/input height + theme | 0.0054 | 0.3% | 0.0111 |
| Composed preparation | 1.0190 | 53.4% | 2.1958 |
| Static shell, participants, transcript, graphics protection | 0.2724 | 14.3% | 0.3528 |
| FX context + admission/pre | 0.0582 | 3.1% | 0.1357 |
| FX closure/pass attachment | 0.0068 | 0.4% | 0.0132 |
| Editor, footer/activity, overlays, stream bubbles | 0.4807 | 25.2% | 0.5047 |
| FX scratch/painter/merge post | 0.0008 | 0.0% | 0.0016 |
| Ratatui buffer diff (includes eager collection) | 0.0528 | 2.8% | 0.0618 |
| TestBackend cell writes | 0.0014 | 0.1% | 0.0067 |

Small residual time is terminal reset/cursor/bookkeeping, timing overhead and the bench's input assertions. Stage p95s must not be added to obtain total p95.

Nested hotspots (already included in the stages above):

| Hotspot | Mean ms/frame | Share of draw | Share within slowest 5% of draws |
|---|---:|---:|---:|
| History scan/state lookup/model comparison | 0.8739 | 45.8% | 57.5% |
| Activity-set synchronization inside draw | 0.4486 | 23.5% | 11.3% |
| Source-anchor rebuilding | 0.1057 | 5.5% | 13.7% |
| Markdown update and prepared prefix/suffix | 0.0358 | 1.9% | 4.7% |

History checks plus in-draw activity synchronization account for **69.3% of mean draw time**. Preparation alone accounts for 76.1% of time within the slowest 5% of draws. Source-anchor work becomes 13.7% in those slow draws. The real editor/overlays/bubbles excluding activity synchronization average only about 0.032 ms.

Outside draw, reducer/input work averages **0.0130 ms** (p95 0.0288); `App::expire` averages **0.6513 ms** (p95 0.8803). The latter includes the **other** history-wide activity scan plus pending-text flush. Total reducer-plus-draw p95 is 3.979 ms. The activity prototype reduces expiry mean to 0.0365 ms and the in-draw activity stage from 0.4486 to about 0.0004 ms; it removes roughly 1.06 ms of mean total work, although its standalone draw-p95 reduction is only 0.223 ms because other stages/tails change. Don't use mean stage savings as additive p95 guarantees.

### Sampling evidence

`perf record -g` used sudo without changing host sysctls (unprivileged access is blocked by `perf_event_paranoid=4`). Two captures excluded initial fixture preparation with delayed recording:

- Hardware cycles, 30-second run, 999 Hz, DWARF/16 KiB stacks: 4,208 samples, zero lost. Self attribution: B-tree MessageId set insertion **36.73%**, `Transcript::state_of` **26.68%**, grapheme iteration **3.24%**, `Composed::prepare` **3.05%**, `sync_activity` **2.09%**, malloc **2.54%**. This is whole-process CPU, including expiry, not draw-only time.
- Software `cpu-clock:u`, 20-second run, 999 Hz, DWARF/65,528-byte stacks: 2,887 samples, zero lost. Self attribution: set insertion **34.50%**, `Composed::prepare` **16.14%**, `state_of` **8.87%**, `sync_activity` **7.41%**, memcmp **4.68%**, grapheme iteration **2.81%**. Instruction-level percentages vary by sampling event and clock, but both identify the same history bookkeeping and agree with the stage timers.

Optimized callchains are incomplete: the first capture had 2,954 empty callchains; the larger software capture had none empty but 1,425 stopped after one frame. Leaf/self attribution and independent wall timers carry the conclusion; do not infer complete ancestry from the flamegraph. The [software flamegraph](fx-draw-perf-research-2026-11-27/composed-clock.svg) is 98 KiB, with its [folded stacks](fx-draw-perf-research-2026-11-27/composed-clock.folded), [software self table](fx-draw-perf-research-2026-11-27/perf-clock-self.txt) and [cycles self table](fx-draw-perf-research-2026-11-27/perf-cycles-self.txt). Large raw captures remain at `/tmp/fx-composed-perf.data` and `/tmp/fx-composed-clock.data` on dev3 and are not committed. `FlameGraph` upstream revision: `41fee1f99f9276008b7cd112fca19dc3ea84ac32`; the supplied `collapse.py` preserves Rust return arrows/generic names.

## Pipeline inspection

- `ui.rs::render` computes the viewport/input layout and calls `Composed::prepare` inside the draw closure. Preparation is not a separate presentation worker in this path. Its signature skips unchanged revisions, but a stream batch invalidates it globally. Each changed revision scans all retained cards, checks membership/supersession, calls `state_of(card.id)` (a new B-tree lookup), looks up the prepared model, compares source/status, then prepares only changed models. This is history-wide checking, not history-wide Markdown parsing every frame.
- `widgets/transcript.rs` already caches Markdown and width, maintains a Fenwick height index, locates the first visible message in O(log N), and paints only clipped rows. `markdown.rs` keeps finalized blocks and reparses only the revisable tail for append updates; durable completion reconciles the full source once. Replacing the renderer with another generic virtualization/cache layer would miss the primary problem.
- `CachedMessage::prepare` still walks all blocks to rebuild source-grapheme anchors after a tail append, calls `source.graphemes(true).count()`, and can instantiate a separate Markdown renderer for changed anchor blocks. It duplicates wrap/parse work and allocates source/row vectors. Preserve finalized block anchor offsets and update the revisable suffix; use direct Markdown source provenance where possible. Do not discard anchors in a product fix.
- `Transcript::sync_activity` extends/inserts the entire history into a BTreeSet on every call. It runs in `App::expire` and again in `ui::render`, even when no card identity changed. Changing text does not require rediscovering all message IDs. `flush` also walks all cards for pending text, and `streaming()` scans until an incomplete card; keep pending/streaming ID sets alongside a dirty set instead.
- Visible rows and spans are cloned by `MarkdownRenderer::visible_rows`, then cloned again in the transcript painter. The composed widget `rows()` builds one owned span per cell, but only when its artifact signature changes, not every steady frame. Reuse borrowed cached lines and merge contiguous equal-style cell runs if this becomes material after fixing scans. Styles are small copied values; focus allocation work on owned strings/vectors and cache retention.
- `ui_fx.rs::attach` clones cue metadata and recreates Pass closures every frame, and `FxManager::reflow_pass` replaces the old Pass (including scratch) even with unchanged geometry. `fx/pass.rs` copies every cell in its clipped rectangle into scratch, runs the painter, then loops over cells to check wide-glyph continuations, `protected.iter().any`, access policy and tier mapping. Some Chrome/Dispatch passes register the full viewport; narrow their regions or use a per-frame protection mask for O(1) cell tests. Retain scratch/painter state across unchanged reflows. There is no steady full-frame `Buffer::clone` in `ui.rs`; scratch cell copies and artifact scratch allocations are the actual costs.
- The event loop already tracks `app.dirty` and schedules effects deadlines. A truly idle room disarms its draw timer. Streaming Full mode asks for animation frames between content updates, so it repeats all the expensive history bookkeeping. Fixing bookkeeping preserves 60 Hz animation; dirty subregion/base-layer reuse is a later optimization. The bench forces 60 Hz even with effects off and does not call `observe_frame`, so it bypasses the production governor's adaptive degradation.

## Gate coverage limitations discovered

`composed_streams` calls `terminal.resize(Rect)` but never resizes TestBackend. `Terminal::draw` calls `autoresize()`, reads the unchanged backend size and restores the original viewport before the callback. The preparation resize counter is effectively zero for the whole run. This gate therefore exercises settlement and buffer clearing at resize intervals, not actual transcript-width reflow. A future harness repair must resize the backend too and separately record resize stalls; full 10,000-card reflow is synchronous today.

The repeating Verdict cue is emitted every eight seconds while follow is true (the scroll pattern repeats every four seconds). Its attached painter lasts 1.8 seconds; the harness scrolls back to the verdict two seconds later. Repeated verdicts are thus clipped offscreen during their active painter window. Full mode is enabled, but this is primarily a stream/Chrome/renderer gate, not continuous visible full-catalogue painting. `fx_frame.rs` instead overlays the catalogue simultaneously at a fixed warm animation time, uses 200x60 scratch passes, and excludes the real transcript and terminal diff. Its 5.12-ms mean is a different workload/statistic and does not explain or bound composed p95.

One extra invalidation detail matters: `Session::reduce` bumps `transcript.revision` on **each accepted Delta**, before its pending text is published by the 50-ms flush. The global composition signature can therefore rescan history while all displayed text remains unchanged. Track publication/layout changes separately from transport/chunk progress. Merely reducing scan frequency helps CPU but need not remove the remaining expensive frames from p95.

## Memory ownership

The fixture retains all 10,000 completed cards by design. `ReplyCard::text`, `TranscriptMessage::source`, sanitized `MarkdownRenderer::source`, `AnchorRows::source`, styled and wrapped Lines, and `FenceHighlighter`'s committed text, highlighted lines and parser state coexist. Rich fence state remains allocated even after a message is complete. Artifact composition also stores owned per-cell spans. Syntect's syntax/theme assets are shared globally through OnceLock, so loading one default syntax set per message is not the mechanism.

Painting is bounded to the viewport, but the cache is not bounded to the viewport: `TranscriptView` retains a CachedMessage for every retained card and has no LRU/byte budget. `Session::page_older` extends accumulated events and restores/reduces the enlarged snapshot. Thus history length drives RSS even though only a few rows are painted. Long sessions can grow with retained history; the fixed-history short runs do not establish a leak. Evict expensive completed-message preparation while retaining identity/height/source anchors, or release incremental fence state after completion, rather than truncating durable messages.

Measured startup / ending RSS, fresh process per case:

| History cards | Startup RSS KiB | End RSS KiB | Window |
|---|---:|---:|---:|
| 0 | 17940 | 20628 | 6 s |
| 500 | 31436 | 33980 | 6 s |
| 10,000 | 252128 | 255656 | 30 s control |
| 10,000, completed fence state released | 184220 | 187156 | 6 s probe |

The 500-to-10,000 startup slope is about **23.2 KiB per historical card**, much more than the ~140-byte source fixture. Releasing only completed fence state returned about **67,908 KiB (66.3 MiB, 27% of startup RSS)** without changing the retained text/styled/wrapped rows. This does not establish exact per-allocation ownership or long-term behavior: RSS includes allocator retention and shared assets, and windows differ in duration. It does demonstrate a substantial avoidable retained parser-state component. A 30-second fixed-history control grew only about 3.4 MiB; neither this nor the supplied 300-second RSS proves a leak. A growing real history still grows the unbounded preparation cache.

## Ranked fixes: saving versus effort

Savings below are planning estimates, not Mac measurements. Mac estimates use 2.6–3.7× Linux for comparable work, and percentile improvements are not additive. The two history prototypes combined reduced Linux p95 by **1.611 ms (52%)**, p99 by **3.481 ms (65%)**, and CPU from **15.595% to 4.866%**. Both retain Full mode and normal widget painting in this fixture.

| Rank | Concrete change and files | Expected saving | Effort / risk |
|---|---|---|---|
| 1 | Track dirty message identities and published presentation versions; `Composed::prepare` upserts only dirty cards. Keep a separate full invalidation for width/epoch/style and artifact revisions. Update dirty identities on flush, completion/failure, retries and supersession, not every pending token. `transcript.rs`, `session.rs`, `composed.rs`, `widgets/transcript.rs`. | Skipping static completed-card checks measured **3.106 → 1.895 ms p95**, saving **1.211 ms Linux**; ~**3.1–4.5 ms Mac** if comparable. Removes the 46% mean history-scan stage rather than just speeding its lookups. | ~1–2 days. Medium risk: missed invalidation, attempt replacement, replay/paging, anchors and terminal outcomes. Add work-count regressions proving O(changed cards), plus existing snapshots. |
| 2 | Update seen/unseen on actual card insertion/follow transitions; avoid both full-history `sync_activity` calls on every tick/draw. Retain pending/streaming ID sets for flush/streaming queries. `transcript.rs`, `session.rs`, `app.rs`, `ui.rs`. | Direct in-draw stage is **0.449 ms mean** (~**1.2–1.7 ms Mac**), plus ~0.615 ms mean outside-draw saving. Standalone prototype's actual p95 saving was **0.223 ms Linux**; CPU **15.595 → 10.232%**. Combining with rank 1 measured **1.495 ms p95**. | Hours to a day. Low–medium risk: follow→pause→follow, clear/restore and paged history must preserve unread counts. The measured cardinality shortcut is not a general membership proof. |
| 3 | Derive CardState from the already-borrowed ReplyCard and known supersession instead of searching the B-tree again. Share a state helper to avoid duplicated rules. `composed.rs`, `transcript.rs`. | Measured **3.106 → 2.806 ms p95**, **0.300 ms Linux**, ~**0.8–1.1 ms Mac**. Helpful interim small patch; largely subsumed by rank 1, so do not add its saving twice. | About an hour. Low risk if terminal/superseded rules share the existing helper and tests. Alone does not safely close the Mac gap. |
| 4 | Retain per-finalized-block grapheme/row anchor provenance; update only the revisable tail. Avoid reparsing each changed block in a temporary anchor MarkdownRenderer. `widgets/transcript.rs`, `markdown.rs`, `text.rs` if shared provenance is needed. | Omitting anchors measured **3.106 → 2.482 ms p95**: **0.624 ms Linux upper bound**. A correct incremental implementation may recover ~0.3–0.6 ms Linux (~**0.8–2.2 ms Mac**) in this fixture; not yet measured. | ~1–2 days. Medium risk: Unicode, tables/reference reconciliation, width changes and stable source anchors. The omission probe deliberately loses anchor correctness. |
| 5 | Release incremental fence parser/highlighter state once durable completion is reconciled; keep prepared styled/wrapped rows. Add a byte budget/LRU for distant completed preparation separately. `markdown.rs`, `markdown/highlight.rs`, later `widgets/transcript.rs` and `composed.rs`. | Completed-fence-state probe saved **66.3 MiB startup RSS**; **no demonstrated steady draw saving**. LRU could reclaim more of ~23 KiB/card but is not measured. | Hours for fence-state lifecycle, days for correct LRU/height handling. Low–medium / medium–high risk respectively; verify references, resize, replacement, copy/search and anchor restoration. |
| 6 | Borrow cached visible Lines rather than cloning twice; cache stable participant display rows; coalesce equal-style artifact spans. `markdown.rs`, `widgets/transcript.rs`, `widgets/participants.rs`, `composed.rs`, `ui.rs`. | Static widgets are 0.272 ms mean total; estimate **0.05–0.15 ms Linux** (~**0.1–0.6 ms Mac**) recoverable, unmeasured. Not the primary miss. | ~1 day. Low–medium risk around Unicode clipping, no-color adaptation and stale participant state. |
| 7 | Retain Pass/scratch across geometry-stable reflows, narrow full-viewport registrations, precompute a protection mask and skip expired/offscreen work before copying. `ui_fx.rs`, `fx/manager.rs`, `fx/pass.rs`, dense painters in `fx/*`. | Current composed FX attachment/post mean is **<0.015 ms** across controls, so even deleting it is insufficient. Full-catalogue benefit is unmeasured; its supplied 5.12-ms mean is an upper bound on the whole workload, not an adapter saving estimate. | ~1–2 days. Medium risk: original cue clock, expiry, protected prose/blanks, wide continuations, color tiers and painter state. Profile the dense catalogue separately before promising savings. |

## Recommended plan

1. Fix the gate coverage first: resize TestBackend along with Terminal, and schedule a visible verdict while its painter is active. Keep existing thresholds; report resize stalls separately as well as in the inclusive latency window. Preserve the current fixture as a historical comparator.
2. Implement dirty-card preparation and event-driven activity accounting together. Include direct card-state derivation within that work. Keep 60 Hz Full animations and the real widget renderer. Regression cases should cover complete/failed/cancelled/retried/superseded messages, follow/unread transitions, replay, paging, width changes and stable anchors. Counters must prove that unchanged history contributes no per-frame lookups/inserts.
3. Use a short Mac run to reprofile, then run the original 300-second gates at all three viewports on hueb with the corrected harness and repeated runs under comparable load. Aim for **≤6 ms p95**, not a marginal 7.9 ms. The combined Linux residual scaled by the observed ratios is roughly **3.9–5.5 ms on Mac**; adding 25% planning reserve gives roughly **4.9–6.9 ms**. This supports the plan, but is not a Mac pass or a guarantee. Check CPU ≤25%, key p95 <50 ms and p99 as well.
4. Release completed fence state for the measured memory win, then optimize incremental anchors if the post-fix profile still needs margin. Retain/limit distant preparation after correctness tests. Optimize full-buffer FX only if a visible dense-effect profile makes it a material residual.

## Reproduction and artifacts

No product source change is committed. [profiling.patch.gz](fx-draw-perf-research-2026-11-27/profiling.patch.gz) applies to the report branch's product sources; `git apply --check` passed after restoring every temporary source edit. It adds viewport selection, stage counters, a timed backend, memory snapshots and explicitly experimental env switches. All prototypes still compile and run with the real bench's typed-character assertions, but **none is a reviewed correctness fix**. The dirty-only probe assumes completed fixture history never changes; the activity probe assumes stable membership when counts match; the anchor omission is intentionally incorrect for scrolling. Use the patch only for research.

```sh
# From this branch's repository root; apply only in a disposable research checkout.
gzip -dc docs/reports/fx-draw-perf-research-2026-11-27/profiling.patch.gz | git apply
CARGO_BUILD_JOBS=4 CARGO_PROFILE_BENCH_DEBUG=1 cargo bench -p squad-tui --bench composed_streams --no-run
# Set BENCH to the composed_streams executable printed above.
COMPOSED_SECS=30 COMPOSED_VIEWPORT=140x42 COMPOSED_ENFORCE=0 "$BENCH"
CARGO_BUILD_JOBS=4 BENCH="$BENCH" python3 docs/reports/fx-draw-perf-research-2026-11-27/run-probes.py
sudo -n env COMPOSED_SECS=30 COMPOSED_VIEWPORT=140x42 COMPOSED_ENFORCE=0 perf record --delay=2500 -F 999 -g --call-graph dwarf,16384 -o /tmp/fx-cycles.data -- "$BENCH"
sudo -n env COMPOSED_SECS=20 COMPOSED_VIEWPORT=140x42 COMPOSED_ENFORCE=0 perf record --delay=2000 -e cpu-clock:u -F 999 -g --call-graph dwarf,65528 -o /tmp/fx-clock.data -- "$BENCH"
```

Recorded profile captures used a preserved uninstrumented binary, `/tmp/fx-base-bench`; the commands above can profile either binary, so label captures accordingly. Profile only one workload at a time and keep build work separate. To reproduce the off comparison use `RESEARCH_FX=off`; completed-state memory probe uses `RESEARCH_DROP_FENCE_STATE=1 COMPOSED_SECS=6`. `RESEARCH_SKIP_HISTORY` is an unused weaker probe option, not part of the reported results.

[Comparison chart](fx-draw-perf-research-2026-11-27/summary.html), raw stdout/stderr, compressed per-frame CSVs (nanoseconds), [timings-summary.json](fx-draw-perf-research-2026-11-27/timings-summary.json), and `analyze.py` are in the [artifact directory](fx-draw-perf-research-2026-11-27/). `analyze.py` accepts decompressed CSV; it summarizes all frames, frames with measured preparation resize under 1 ms, and the slowest 5% of draws. In these runs the first two groups are identical because resize did not reach preparation. The runner executes probes sequentially. Formatting and diff whitespace checks pass; no full workspace suite was run because the committed change contains research documents, scripts and an unapplied patch only.
