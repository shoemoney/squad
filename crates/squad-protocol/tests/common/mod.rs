#![allow(dead_code)]
use serde_json::{Value, json};
pub const ID: &str = "01900000-0000-7000-8000-000000000001";
pub const TIME: &str = "2026-11-24T12:00:00Z";
pub fn envelope(kind: &str, payload: Value) -> Value {
    let sender = match kind {
        "user.message" => "me",
        "agent.reply" | "message.final" | "delta" | "debate.turn" => "a",
        "verdict" | "debate.round_score" | "case.ruling" => "judge",
        _ => "system",
    };
    json!({"version":1,"id":ID,"roomId":"main","correlationId":ID,"sender":sender,"recipients":["me"],"createdAt":TIME,"kind":kind,"payload":payload})
}
pub fn generation() -> Value {
    json!({"messageId":ID,"workId":ID,"triggerEventId":ID,"attempt":1})
}
pub fn completed() -> Value {
    let mut v = generation();
    v.as_object_mut().unwrap().extend(
        json!({"model":"vendor/model","finishReason":"stop","usage":null})
            .as_object()
            .unwrap()
            .clone(),
    );
    v
}
pub fn completion() -> Value {
    let mut v = completed();
    v.as_object_mut().unwrap().extend(
        json!({"inReplyToMessageId":ID,"content":"answer"})
            .as_object()
            .unwrap()
            .clone(),
    );
    v
}
pub fn estimate() -> Value {
    json!({"estimatedUsd":1.0,"reservedUsd":2.0,"remainingUsd":20.0,"modelCalls":2})
}
pub fn setup() -> Value {
    json!({"debateId":ID,"topicMessageId":ID,"topicText":"topic","solutions":[{"nick":"a","messageId":ID,"text":"one"},{"nick":"b","messageId":ID,"text":"two"}],"instruction":"","rounds":1,"judgeModel":"vendor/judge","judgeSwapped":false})
}
pub fn judge() -> Value {
    let scores=["a","b"].map(|nick|json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|c|json!({"criterionId":c,"score":5,"reasoning":"reason"})))}));
    json!({"winner":"a","scores":scores,"reasoning":"instruction breaks the tie","whatToStealFromLoser":[],"tallyOverrideReason":null})
}
pub fn verdict() -> Value {
    let mut v = completed();
    v.as_object_mut()
        .unwrap()
        .extend(judge().as_object().unwrap().clone());
    v.as_object_mut().unwrap().extend(json!({"debateId":ID,"caseId":ID,"judgeInvocationId":ID,"rubricVersion":"v1","weightedTotals":[{"nick":"a","total":5.0},{"nick":"b","total":5.0}],"roundTally":{"a":0,"b":0}}).as_object().unwrap().clone());
    v
}
pub fn fixtures() -> Vec<(&'static str, Value)> {
    let mut turn = generation();
    turn.as_object_mut().unwrap().extend(json!({"debateId":ID,"round":1,"speaker":"a","outcome":{"status":"completed","content":"turn","model":"vendor/model","finishReason":"stop","usage":null}}).as_object().unwrap().clone());
    let mut ruling = completed();
    ruling.as_object_mut().unwrap().extend(
        json!({"caseId":ID,"judgeInvocationId":ID,"inReplyToEventId":ID,"content":"ruling"})
            .as_object()
            .unwrap()
            .clone(),
    );
    let mut delta = generation();
    delta.as_object_mut().unwrap().extend(
        json!({"chunkIndex":0,"text":"hi"})
            .as_object()
            .unwrap()
            .clone(),
    );
    vec![
        (
            "user.message",
            json!({"messageId":ID,"text":"@a hi","body":"hi","resolvedQuotes":[]}),
        ),
        ("agent.reply", completion()),
        ("message.final", completion()),
        ("delta", delta),
        (
            "presence",
            json!({"nick":"a","status":"idle","queueDepth":0,"observedAt":TIME}),
        ),
        ("system.notice", json!({"code":"READY","text":"ready"})),
        (
            "error",
            json!({"code":"DAILY_MAX_REACHED","text":"Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.","retryable":false}),
        ),
        (
            "var.change",
            json!({"name":"ROUNDS","value":1,"previousValue":10}),
        ),
        (
            "work.state",
            json!({"workId":ID,"triggerEventId":ID,"target":"a","contextId":"a","attempt":0,"status":"queued"}),
        ),
        (
            "usage.record",
            json!({"workId":ID,"attempt":1,"target":"a","contextId":"a","model":"vendor/model","startedAt":TIME,"outcome":"unknown","usage":null,"responseModel":null,"day":"2026-11-24"}),
        ),
        (
            "budget.state",
            json!({"spentUsd":0.0,"reservedUsd":1.0,"limitUsd":20.0,"estimatedUsd":1.0,"unresolvedAttempts":0,"day":"2026-11-24","unknownCalls":0}),
        ),
        (
            "debate.preflight",
            json!({"debateId":ID,"planId":ID,"expiresAt":TIME,"estimate":estimate(),"confirmationRequired":false,"debaters":["a","b"],"rounds":1,"judgeRequested":false,"openingRoundRequired":false}),
        ),
        (
            "debate.preflight.result",
            json!({"debateId":ID,"planId":ID,"status":"confirmed","confirmation":"automatic"}),
        ),
        (
            "debate.opening",
            json!({"debateId":ID,"debaters":["a","b"],"topicMessageId":ID,"topicText":"topic","instruction":"","rounds":1,"round":0}),
        ),
        ("debate.setup", setup()),
        (
            "contest.transition",
            json!({"contestId":ID,"kind":"debate","expectedStateVersion":0,"transition":{"kind":"debate","transition":{"kind":"prepare"}}}),
        ),
        (
            "debate.state",
            json!({"debateId":ID,"state":"PREPARING","debaters":["a","b"],"round":0,"totalRounds":1,"judgeModel":"vendor/judge","judgeRequested":true,"stateVersion":0,"terminalReason":null,"caseId":null,"caseIds":[]}),
        ),
        ("debate.turn", turn),
        ("verdict", verdict()),
        (
            "case.opened",
            json!({"caseId":ID,"judgeInvocationId":ID,"judgeModel":"vendor/judge","material":{"kind":"standalone","messages":[{"eventId":ID,"author":"a","text":"answer"}],"question":"which?"}}),
        ),
        (
            "case.followup",
            json!({"caseId":ID,"userEventId":ID,"judgeInvocationId":ID,"workId":ID}),
        ),
        ("case.ruling", ruling),
        (
            "message.superseded",
            json!({"supersededEventId":ID,"supersededMessageId":ID,"member":"a","inReplyToMessageId":ID,"replacementWorkId":ID,"replacementMessageId":ID}),
        ),
        (
            "scores.changed",
            json!({"verdictEventId":ID,"scores":[{"nick":"a","wins":1,"losses":0,"trophies":0}]}),
        ),
        ("room.cleared", json!({"epoch":2})),
        (
            "debate.round_score",
            json!({"debateId":ID,"caseId":ID,"round":1,"scores":{"a":{"score":5,"reason":"Good."},"b":{"score":5,"reason":"Good."}},"tally":{"a":5,"b":5}}),
        ),
        (
            "contest.setup",
            json!({"contestId":ID,"kind":"debate","scriptVersion":"v1","participants":["a","b"],"sourceEventIds":[],"settings":{}}),
        ),
        (
            "contest.state",
            json!({"kind":"generic","state":{"contestId":ID,"kind":"debate","stateVersion":0,"status":"PREPARING","artifactEventIds":[],"terminalReason":null}}),
        ),
        (
            "contest.preflight",
            json!({"contestId":ID,"kind":"grill","planId":ID,"expiresAt":TIME,"estimate":estimate(),"participants":["a","b"],"confirmationRequired":false,"settings":{}}),
        ),
        (
            "contest.preflight.result",
            json!({"contestId":ID,"planId":ID,"status":"cancelled"}),
        ),
        (
            "contest.step",
            json!({"contestId":ID,"stepId":ID,"ordinal":1,"scriptVersion":"v1","moveName":"barrier","inputs":[],"assignments":[]}),
        ),
        (
            "contest.artifact",
            json!({"contestId":ID,"stepId":ID,"artifactKind":"sample","sourceRefs":[],"data":{}}),
        ),
        (
            "contest.report",
            json!({"contestId":ID,"stepId":ID,"artifactEventIds":[],"skippedWorkIds":[],"data":{}}),
        ),
        (
            "contest.collecting",
            json!({"contestId":ID,"kind":"debate","questionMessageId":ID,"question":"why?","addressed":["a","b"]}),
        ),
        (
            "contest.pick_requested",
            json!({"contestId":ID,"kind":"debate","questionMessageId":ID,"eligible":["a","b"],"requiredPicks":2,"banner":"Pick two"}),
        ),
        (
            "contest.picked",
            json!({"contestId":ID,"kind":"debate","nicks":["a","b"],"sourceMessageIds":[ID,"01900000-0000-7000-8000-000000000002"]}),
        ),
    ]
}
