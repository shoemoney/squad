# v0.2.0 integration live evidence

Runs use the default six-member roster, daily max 150, one paid invocation per
mode, fresh private data, and `/dev/null` as the env file. Credentials are fetched
with the dev1 `aigate-key openrouter` helper and captured directly into the child
environment. No real env file or credential is saved. Raw artifacts are scanned
for the credential value before a summary is reported.

## Tested snapshot

The runs below use source `c7c20ff08600246606effe917e5eb7ab7adeac8c`, version
0.2.0. The binary is snapshotted at `/tmp/squad-v020-build-c7c20ff/squad`; its SHA-256
is recorded in `build.json` there and each run's `summary.json`. Resolved default
members are Claude Opus 5.5, Claude Fable 5.2, GPT-6.2 Sol, Gemini 4 Neon, Grok 4.7
and Muse Wonder; the judge is Claude Fable 5.2. Each summary preserves the full
resolved-model cache and frozen preflight.

| Mode | CLI / contest result | Spend incl. startup | Wall | Initial / peak reserved | Final reserved / unknown | Evidence on dev2 |
|---|---|---:|---:|---:|---|---|
| Rank | 0 / COMPLETE, ranked | $0.190184 | 38.636 s | $18.024576 / $18.024576 | $0 / 0 | `/tmp/squad-v020-rank-lzd2qyl3/` |
| Forge | 0 / COMPLETE, original_won | $0.345117 | 80.682 s | $24.736836 / $24.736836 | $0 / 0 | `/tmp/squad-v020-forge-xtnz6pme/` |
| Forecast | 0 / COMPLETE, complete | $0.470614 | 105.725 s | $20.188827 / $20.188827 | $0.000000 / 0 | `/tmp/squad-v020-forecast-7k6pluyb/` |
| Decide | 0 / COMPLETE, complete | $0.400975 | 69.135 s | $16.400079 / $16.400080 | $0.000000 / 0 | `/tmp/squad-v020-decide-v5zzu1x4/` |
| Blow It Up | 0 / COMPLETE, reported | $1.469007 | 265.024 s | $20.574231 / $20.574231 | $0.000000 / 0 | `/tmp/squad-v020-blow_it_up-sz7xh2_2/` |
| Readback | 0 / AWAITING_HUMAN, choices required | $0.428807 | 57.114 s | $10.961076 / $10.961076 | $6.724861 / 0 | `/tmp/squad-v020-readback-qpox1sy9/` |
| Tune | 0 / COMPLETE, tuned | $0.522077 | 134.272 s | $24.970323 / $24.970323 | $0.000000 / 0 | `/tmp/squad-v020-tune-voxbepir/` |
| Bass Ackwards PRD | 0 / COMPLETE, saved | $2.552690 | 410.874 s | $50.711781 / $50.711781 | $0.000000 / 0 | `/tmp/squad-v020-bass_ackwards_prd-rklsr3wq/` |

Rank used fifteen pairs, replication three and forty-five judgments. All six
batches were accepted; Claude's first batch needed its one private validation
repair. Eight Rank attempts and four startup probes all settled. Offline export
passed. The top item was recovery without duplicate provider calls.

Forge used all fifteen normal mode calls without retries, plus four startup
probes. It returned the best original verbatim, with no judge failure. Credential-free
offline export passed.

All eight enabled additions have had one integrated invocation. Seven reached
`COMPLETE`; Readback returned zero at its documented human pause. None ended
`judge_failed`. Trace, Debug, Negotiate and the rendering-performance branch
await the owner's merge notice.

## Delayed receipt repair on the original evidence

The integrated runtime reopened Decide's original dev2 room and Forecast's
original dev3 room with `/who`. Both commands exited zero, with unchanged
nineteen-attempt counts. Decide settled its $0.014226 receipt to $0.621906 spent;
Forecast settled $0.014302 to $0.507324. Both now have zero reservations and unknown
calls. Before-state SQLite backups are retained with the reconciliation summaries:
`/tmp/squad-v020-reconcile-decide-jmsxp55j/` on dev2 and
`/tmp/squad-v020-reconcile-forecast-cvbelkrb/` on dev3.

The integrated Blow It Up run retained five accepted attacks and five challenges.
Grok's attack was refused on both attempts; its challenge timed out twice.
Another member used one private repair. The judge still completed the report
without `judge_failed`. Both timeout receipts reconciled during the run; no
reopen or manual ledger edit was needed. Sixteen mode attempts and four startup
probes settled to $1.469007.

Readback exited at its documented headless human pause after six accepted
interpretations and a diff, with one private member repair. Its $6.724861
reservation covers unstarted continuation work; no charge is unknown. No human
choice was fabricated. The report-only offline export currently returns
`contest not found` until a terminal report exists. Human continuation and its
final export remain open.

Bass Ackwards PRD dispatched twenty-nine mode attempts against the twenty-eight
normal-call plan. The finalizer's first answer exceeded the 2,400-word prose
budget; its one private validation repair passed. All other work completed on
the first attempt. The 25,780-byte PRD was saved under the run's `data/exports/`,
and the emitted `export.ready` path exists. Offline export passed. Final holds
and unknown calls were zero.

## Follow-up source validation

The startup receipt-owner guard at `8bade6d08529e9d688141ebde576cae6cd4d57cc`
passed all required workspace gates and an explicit release build. Its private
binary snapshot and SHA-256 are in `/tmp/squad-v020-build-8bade6d/build.json`.
The paid invocations above retain the earlier integrated source and binary hash.
Total observed spend across the eight fresh smoke rooms is $6.379471.

The saved PRD path is `/tmp/squad-v020-bass_ackwards_prd-rklsr3wq/data/exports/prd-self-hosted-status-page-with-outage-alerts-01a21b55-edfa-73da-8505-e8c9ef373546.md` (mode 0600,
SHA-256 `6c36d8f59f9ed41eea72ef0ed9e3443ce5264a179c4608ddac93c9bde0c0360e`). Its content occurs verbatim in the full evidence
export. File metadata is retained in the run's `saved-path.json`.
