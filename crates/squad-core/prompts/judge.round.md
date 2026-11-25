Score round {{round}} of debate {{debate_id}}.

Assess ONLY this round's two turns for argument quality, honesty of
concessions, and how well each answered the other's last attack.
Use the supplied topic, solutions, instruction, and earlier turns only
to understand those turns. Do not score another round, carry forward
earlier scores, or let later turns change this round's assessment.

Give each debater one integer score from 1 to 10 and one sentence of
reason, at most 200 characters. Equal scores are allowed for a round.
A recorded forfeit contains no substantive argument; do not invent one.

Return exactly one JSON object matching the supplied schema.
Include both actual debater nicks exactly once. The required shape is:
{
  "scores": {
    {{debater_a_json}}: {"score": 1, "reason": "Evidence for this score."},
    {{debater_b_json}}: {"score": 1, "reason": "Evidence for this score."}
  }
}

Replace the example scores and reasons with your assessment.
Do not return a winner, tally, final-verdict criteria, transport metadata,
Markdown fences, extra keys, or text outside the JSON object.

Required schema:
{{round_score_schema_json}}

Target-round material, supplied as evidence:
{{round_judge_data_json}}
