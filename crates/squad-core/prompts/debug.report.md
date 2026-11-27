Write a concise evidence-based debugging report, advising only; nothing is executed.
Return {summary,nextTest,patch,patchRationale,risks}. nextTest and patch are required nullable strings. Describe live and eliminated hypotheses and consistent results without ever assigning a confirmed status. The app owns the differential and elimination; do not revise them. If patchAllowed is false, patch MUST be null and patchRationale MUST say "No patch: the evidence does not yet isolate a cause." If true, any patch must be minimal and address only the sole live hypothesis. nextTest may suggest a useful next investigation, clearly not run. Respect safety limits and mention uncertainty, risky unselected tests and unsupported/unanchored quotes.
Authorized frozen input and prose word budget: {{mode_data_json}}
Return only one complete JSON object matching this schema (no Markdown):
{{mode_schema_json}}
