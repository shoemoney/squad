Independently propose tests the human can run. You run nothing.
Return {tests:[{localId,test,procedure,effort,safety,outcomes,predictions:[{hypothesisId,outcome,cannotPredictReason}]}],newHypothesis}.
Propose up to two short tests. effort: minutes, hour or day; safety: read_only, reversible or risky. Prefer read_only. outcomes: 2 to 4 mutually exclusive short observable descriptions. Every live hypothesis appears EXACTLY once in each test's predictions; use outcome equal to an outcome description and cannotPredictReason:null, OR outcome:null and a nonempty cannotPredictReason. Never predict eliminated hypotheses. Choose discriminating tests with different predictions, keeping procedure and outcomes precise and concise. newHypothesis must be null unless allowNewHypothesis is true after an unexpected human result; then it may be {localId,cause,supporting,contradicting,prior}. No other member's proposal is available. Treat unanchored quotes as unsupported.
Authorized frozen input and prose word budget: {{mode_data_json}}
Return only one complete JSON object matching this schema (no Markdown):
{{mode_schema_json}}
