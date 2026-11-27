Merge the labelled hypothesis tables, preserving every source, disagreement and minority cause.
Return {hypotheses:[{hypothesisId,cause,sources,supporting,contradicting,priorSpread}]} with 1 to 8 hypotheses and short unique H1, H2, ... IDs. Each sources entry is exactly "Table n:localId". Partition ALL input sources exactly once, merging duplicates without dropping minority hypotheses. Preserve each source's supporting and contradicting quotes exactly, deduplicated within each array; preserve the distinct priors as priorSpread. Unanchored quotes are retained for audit and never evidence. Do not rank to a cause, assign status, eliminate, add causes or propose a test. Keep prose concise so the complete JSON fits the token budget.
Authorized frozen input and prose word budget: {{mode_data_json}}
Return only one complete JSON object matching this schema (no Markdown):
{{mode_schema_json}}
