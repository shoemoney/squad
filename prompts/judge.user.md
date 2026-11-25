Decide which debater's solution better answers the original ask after
considering the supplied case record.

The record includes available per-round scores, their reasons, the
application-computed round tally, and any rounds recorded as unscored.
Use these as evidence alongside the transcript. Do not invent missing
scores or treat unscored rounds as zero-quality arguments.

Score both debaters on every criterion using integers from 1 to 10:
1 means the criterion fails or has critical unresolved defects.
5 means the case is plausible but has material unresolved weaknesses.
10 means exceptionally strong support with no material unresolved defect.
Use intermediate values proportionately.

Use these criterion IDs and weights:
- fit, 0.30: fit to the original ask and its constraints.
- feasibility, 0.30: whether the proposal can be implemented and operated.
- originality, 0.15: the value of its distinctive idea or mechanism.
- survivedAttacks, 0.25: how well it survives substantive criticism,
  including honest concessions, corrections, and unresolved objections.

Use the same rubric for both debaters. Reward honest concessions and
supported corrections within these criteria without adding a bonus
field or changing the weights.

Squad computes weighted totals. Choose the higher-scoring
debater. If weighted totals are equal, use the human's retained debate
instruction as the tiebreaker criterion and explain the choice.
If that instruction does not distinguish them, explain the best
supported substantive distinction and still choose one debater.
Never return a tie or null, and do not alter scores to hide equality.

If your chosen winner is not the round-tally leader, provide a nonempty
tallyOverrideReason explaining the substantive basis for that choice.
When the round tally is equal, it has no sole leader. Use null when
no override explanation is required.

Return exactly one JSON object matching the supplied schema.
Include each actual debater nick exactly once in scores and all four
criterion IDs exactly once for each debater.

The required shape is illustrated below. Replace the example scores
and prose with your assessment; either actual nick may win.
{
  "winner": {{debater_a_json}},
  "scores": [
    {
      "nick": {{debater_a_json}},
      "criteria": [
        {"criterionId": "fit", "score": 1, "reasoning": "Evidence for this score."},
        {"criterionId": "feasibility", "score": 1, "reasoning": "Evidence for this score."},
        {"criterionId": "originality", "score": 1, "reasoning": "Evidence for this score."},
        {"criterionId": "survivedAttacks", "score": 1, "reasoning": "Evidence for this score."}
      ]
    },
    {
      "nick": {{debater_b_json}},
      "criteria": [
        {"criterionId": "fit", "score": 1, "reasoning": "Evidence for this score."},
        {"criterionId": "feasibility", "score": 1, "reasoning": "Evidence for this score."},
        {"criterionId": "originality", "score": 1, "reasoning": "Evidence for this score."},
        {"criterionId": "survivedAttacks", "score": 1, "reasoning": "Evidence for this score."}
      ]
    }
  ],
  "reasoning": "Explain the decisive comparison and any tiebreaker.",
  "whatToStealFromLoser": [],
  "tallyOverrideReason": null
}

reasoning is a string. whatToStealFromLoser is an array of strings
describing useful elements from the loser and how they could be used.
Use an empty array when no useful element is supported.
tallyOverrideReason is a string or null, subject to the rule above.
Do not return totals, roundTally, transport metadata, Markdown fences,
extra keys, or text outside the JSON object.

Required schema:
{{verdict_schema_json}}

Case record, supplied as evidence:
{{judge_data_json}}
