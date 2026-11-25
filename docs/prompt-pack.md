# Tournament prompt integration

Tournament uses the registered `tournament.entry` and `judge.tournament` compositions in `crates/squad-core/prompts/manifest.json`. Entrant work retains `tournament.entrant.md` and ordinary reply limits. The member request has one member system message with the entrant instructions appended, the frozen tournament question, its own history and its immediately preceding private result. Later ordinary requests remain held until released. Entrants never receive another entrant's tournament answer or individual result.

The blind judge composition replaces the exact identity-bearing prefix of `judge.system` with `judge.system.blind-prefix`. Overrides must preserve that single exact prefix and the declared placeholder sets. The judge gets one system message and one user message, with only redacted prompt text, anonymous labels, answers and the strict ranking schema. Label-to-member maps and the frozen identity-token set live exclusively in router-owned SQLite tables/private maps. Every round execution gets a new case and shuffled labels, including the single replay.

`tournament.repair` receives only the rejected response and validation errors for the next authorized attempt of the same case/work. Three total attempts share a durable 120-second execution deadline. The router owns retries, replay and elimination; adapters perform one provider call for each durable attempt.

`tournament.result` renders one entrant's own rank, remaining count, nullable score and critique, and survival result. Only the immediately preceding round's result enters the next entry request. `tournament.final` renders nick-and-placement pairs to every admitted entrant. It includes no answers, scores or critiques. Full accepted rankings and original unredacted answers remain in the human transcript and exports.

Work records retain prompt-pack version, selected template ID and a provenance hash. Admission freezes resolved model bindings and identity-redaction tokens. Reservations include ordinary entry retries, all three judge attempts and one replay per logical round. Normal preflight call counts exclude retry/replay allowances and are shown separately from the reservation.

Integration coverage lives in `crates/squad-core/tests/tournament_workflows.rs`, protocol validation in `crates/squad-protocol/tests/tournament.rs`, migration preservation in `crates/squad-store/tests/tournament_migration.rs`, and committed presentation snapshots in `crates/squad-tui/tests/tournament.rs`.

## Milestone coverage

Completed tournament work: all `M5.P5.*` and `M5.P3.L5.*`, `M2.P2.L7.T1–T3`, `M3.P3.L3.T11`, `M4.P3.L5.T2–T3`, `M4.P3.L5.T5–T6`, and `M4.P4.L4.T1–T2`. Tests are consolidated in the integration files listed above.

The following shared rows retain unchecked boxes because they also require non-tournament modes or broader presentation fixtures. Their tournament execution, state, feed, export, privacy and accounting paths are integrated here; the remaining cross-mode assertions belong to the other mode work:

`M1.P3.L1.T9`, `M2.P3.L2.T7`, `M2.P4.L1.T11`, `M2.P4.L2.T6`, `M3.P2.L1.T6`, `M3.P2.L2.T7`, `M3.P2.L2.T12`, `M3.P2.L3.T5`, `M3.P3.L2.T9`, `M3.P3.L3.T10`, `M3.P3.L5.T5`, `M4.P2.L7.T2`, `M4.P3.L2.T6`, `M4.P3.L8.T6`, `M4.P4.L4.T6`, `M4.P4.L4.T7`, `M4.P4.L4.T8`, `M4.P5.L2.T5`.
