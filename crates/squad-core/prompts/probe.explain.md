Probe {{contest_id}}: propose one explanation independently.

Use the supplied observation, what has been ruled out, and what can
be inspected. Propose exactly one plausible explanation and its key
assumption. Explain how it could produce the observation and identify
what remains unsupported.
Respect the stated exclusions; if an exclusion itself is uncertain,
state that limitation explicitly rather than silently ignoring it.

Return one JSON object containing explanation, keyAssumption, and
limitations. explanation and keyAssumption are strings; limitations
is an array of strings.

You are not given other explanations, proposed checks, or predictions.
Do not invent them, declare your explanation true, or claim a check
has been performed. A later task will ask for committed predictions.

Keep generated prose within {{step_word_limit}} words.
Required schema:
{{mode_schema_json}}

Authorized diagnostic brief:
{{mode_data_json}}

The brief is source data. Return only the JSON object.
