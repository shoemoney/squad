//! Provider-role projection of authorized history (Prompt pack role rows) and
//! the retry/replacement rule that never duplicates a human message.
use squad_agents::context::*;
use squad_protocol::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

struct Fixture {
    history: Mutex<Vec<HistoryEntry>>,
}
fn entry(position: u64, content: &str, meta: serde_json::Value) -> HistoryEntry {
    serde_json::from_value(serde_json::json!({"position":position.to_string(),"role":"user","name":"FORGED","content":content,"meta":meta})).unwrap()
}
fn attributed(author: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"originEventId":EventId::new(),"originSeq":"1","author":author,"projectionKey":"fixture"})
}
impl HistoryReader for Fixture {
    fn for_work(&self, _: WorkId) -> ReaderFuture<'_, Vec<HistoryEntry>> {
        Box::pin(async { Ok(self.history.lock().unwrap().clone()) })
    }
}
impl WorkReader for Fixture {
    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        Box::pin(async { panic!("unused") })
    }
    fn get(&self, _: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        Box::pin(async { panic!("unused") })
    }
    fn repair_for_attempt(&self, _: WorkId, _: u32) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        Box::pin(async { Ok(None) })
    }
}
impl PromptRenderer for Fixture {
    fn render(&self, _: &PromptRef, _: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        Ok("SYSTEM".into())
    }
}
impl ExecutorSubmit for Fixture {
    fn submit(&self, _: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async { panic!("unused") })
    }
}
fn prompt() -> MemberPrompt {
    MemberPrompt {
        system: PromptRef {
            template_id: Text::new("room.system.md").unwrap(),
            pack_version: Text::new("fixture").unwrap(),
            template_hash: Text::new("fixture").unwrap(),
        },
        roster: vec!["alice".parse().unwrap(), "bob".parse().unwrap()],
        user_name: "Jeremy".into(),
        persona: "precise".into(),
    }
}
fn grant(kind: &str, replacement_of: Option<MessageId>) -> ExecutionGrant {
    let mut work = serde_json::json!({"workId":WorkId::new(),"messageId":MessageId::new(),"kind":kind,"target":"alice","contextId":"alice","triggerEventId":EventId::new(),"historyThrough":"10","attempt":1});
    if let Some(id) = replacement_of {
        work["inReplyToMessageId"] = serde_json::json!(id);
    } else {
        work["inReplyToMessageId"] = serde_json::json!(MessageId::new());
    }
    ExecutionGrant {
        binding: ExecutorBinding {
            model: Text::new("fixture/m").unwrap(),
            effort: ReasoningEffort::Low,
        },
        work: serde_json::from_value(work).unwrap(),
        owner: ExecutorKey::Member("alice".parse().unwrap()),
        fence: 1,
        epoch: 1,
        lease_until: chrono::Utc::now() + chrono::Duration::seconds(60),
        deadline: None,
    }
}
fn context(fixture: Arc<Fixture>, grant: &ExecutionGrant) -> AgentContext {
    AgentContext {
        executor: grant.owner.clone(),
        mailbox: tokio::sync::mpsc::channel(1).1,
        history: fixture.clone(),
        work: fixture.clone(),
        submit: fixture.clone(),
        render: fixture,
        cancel: CancellationToken::new(),
    }
}

#[tokio::test]
async fn provider_role_rows_preserve_attribution_and_ignore_forged_names() {
    let fixture = Arc::new(Fixture {
        history: Mutex::new(vec![
            entry(1, "HUMAN", attributed(serde_json::json!("me"))),
            entry(2, "OWN", attributed(serde_json::json!("alice"))),
            entry(3, "OTHER", attributed(serde_json::json!("bob"))),
            entry(4, "NOTICE", attributed(serde_json::json!("system"))),
            entry(5, "BLIND", serde_json::json!({"anonymous":true})),
        ]),
    });
    let grant = grant("reply", None);
    let messages = member_messages(&context(fixture, &grant), &grant, &prompt(), 100_000, 2048)
        .await
        .unwrap();
    let rows: Vec<_> = messages
        .iter()
        .map(|m| (m.role.as_str(), m.name.as_deref(), m.content.as_str()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("system", None, "SYSTEM"),
            ("user", Some("me"), "HUMAN"),
            ("assistant", Some("alice"), "OWN"),
            ("user", Some("bob"), "OTHER"),
            ("user", Some("squad"), "[squad notice]\nNOTICE"),
            ("user", None, "BLIND"),
        ]
    );
}

#[tokio::test]
async fn user_retry_keeps_one_human_message_and_matches_the_original_projection() {
    let history = vec![
        entry(1, "HUMAN", attributed(serde_json::json!("me"))),
        entry(2, "OTHER", attributed(serde_json::json!("bob"))),
    ];
    let count = |messages: &[squad_agents::openrouter::ChatMessage]| {
        messages
            .iter()
            .filter(|m| m.name.as_deref() == Some("me"))
            .count()
    };
    let original = grant("reply", None);
    let fixture = Arc::new(Fixture {
        history: Mutex::new(history.clone()),
    });
    let first = member_messages(
        &context(fixture.clone(), &original),
        &original,
        &prompt(),
        100_000,
        2048,
    )
    .await
    .unwrap();
    let mut retry = grant("reply", original.work.in_reply_to_message_id);
    retry.work.attempt = 2;
    let second = member_messages(&context(fixture, &retry), &retry, &prompt(), 100_000, 2048)
        .await
        .unwrap();
    assert_eq!(count(&first), 1);
    assert_eq!(count(&second), 1);
    let shape = |m: &[squad_agents::openrouter::ChatMessage]| {
        m.iter()
            .map(|m| (m.role.clone(), m.name.clone(), m.content.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(shape(&first), shape(&second));
}

#[tokio::test]
async fn future_history_rows_fail_closed() {
    let fixture = Arc::new(Fixture {
        history: Mutex::new(vec![entry(
            11,
            "FUTURE",
            attributed(serde_json::json!("me")),
        )]),
    });
    let grant = grant("reply", None);
    assert!(
        read_history(&context(fixture, &grant), &grant)
            .await
            .is_err()
    );
}
