Build a blind test proposal for the supplied prompt and task. Return only the JSON object in the schema. Propose 1–3 cases and 1–3 rubric criteria, using short unique localIds. Include typical, edge or adversarial inputs that distinguish good behavior from plausible failures. Each input is at most 150 words; 1–3 expect lines and 0–2 mustNot lines, each at most 25 words. Criterion names are at most 6 words, descriptions 40. Keep the entire proposal within the room word budget. Do not answer the cases or rewrite the prompt.
Authorized input: {{mode_data_json}}
Schema: {{mode_schema_json}}
