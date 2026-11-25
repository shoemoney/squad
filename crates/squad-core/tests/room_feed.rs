#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn stalled_feed_recovers_committed_events_from_the_last_seen_cursor() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    let mut feed = room.subscribe();
    let first = accepted(room.submit(set(3)).await);
    let FeedItem::Committed(event) = feed.recv().await.unwrap() else {
        panic!()
    };
    assert_eq!(event.commit.seq, first);
    for _ in 0..constants::FEED_CAPACITY + 1 {
        accepted(room.submit(set(4)).await);
    }
    assert!(matches!(
        feed.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ));
    let mut cursor = first;
    let mut count = 0;
    loop {
        let page = room.events_after(Some(cursor), 500).await.unwrap();
        if page.is_empty() {
            break;
        }
        for event in page {
            assert!(event.commit.seq > cursor);
            event.validate().unwrap();
            cursor = event.commit.seq;
            count += 1;
        }
    }
    assert_eq!(count, constants::FEED_CAPACITY + 1);
    room.shutdown().await.unwrap();
}

// ---- M3.P2.L3.T3 / T4 / T5 / T7 ----
#[path = "support/m6_room.rs"]
mod live;
use live::fake_openrouter::*;
use std::time::Duration;
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

fn drain(feed: &mut FeedReceiver) -> Vec<FeedItem> {
    let mut items = Vec::new();
    while let Ok(item) = feed.try_recv() {
        items.push(item);
    }
    items
}

/// Drain the feed once it has delivered everything durable history holds: the
/// feed is published after commit, so a database condition alone can run ahead.
async fn drain_settled(
    feed: &mut FeedReceiver,
    room: &squad_core::room::RoomHandle,
) -> Vec<FeedItem> {
    let mut items = Vec::new();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            items.extend(drain(feed));
            let delivered = committed(&items).last().map(|e| e.commit.seq);
            let last = room
                .page_events(None, 500)
                .await
                .unwrap()
                .last()
                .map(|e| e.commit.seq);
            if delivered >= last {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("feed did not catch up with committed history");
    items
}

fn committed(items: &[FeedItem]) -> Vec<CommittedEnvelope> {
    items
        .iter()
        .filter_map(|i| match i {
            FeedItem::Committed(e) => Some(e.clone()),
            _ => None,
        })
        .collect()
}

/// Everything the feed delivered as committed must equal durable history, in order.
fn assert_feed_matches_history(feed: &[CommittedEnvelope], history: &[CommittedEnvelope]) {
    assert!(
        feed.windows(2).all(|w| w[0].commit.seq < w[1].commit.seq),
        "feed order is commit order"
    );
    let durable: Vec<_> = history
        .iter()
        .filter(|h| feed.iter().any(|f| f.base.id == h.base.id))
        .cloned()
        .collect();
    assert_eq!(feed, durable.as_slice());
}

#[tokio::test]
async fn higher_attempt_replacement_deltas_precede_the_ordered_final() {
    let f = live::Fixture::new().await;
    f.fake
        .completions(vec![disconnected(), streaming("replacement text", "stop")])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    room.submit(f.request("@member0 stream with a retry")).await;
    f.finals(1).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let items = drain(&mut feed);
    let deltas: Vec<&Delta> = items
        .iter()
        .filter_map(|i| match i {
            FeedItem::Delta(ProducerEnvelope {
                content: EnvelopePayload::Delta(d),
                ..
            }) => Some(d),
            _ => None,
        })
        .collect();
    assert!(!deltas.is_empty());
    // One message identity throughout; attempts never go backwards and chunks restart per attempt.
    assert!(
        deltas
            .iter()
            .all(|d| d.generation.message_id == deltas[0].generation.message_id)
    );
    assert!(
        deltas
            .windows(2)
            .all(|w| w[0].generation.attempt <= w[1].generation.attempt)
    );
    let top = deltas.iter().map(|d| d.generation.attempt).max().unwrap();
    assert_eq!(top, 2, "the replacement is the higher attempt");
    let first_of_top = deltas.iter().find(|d| d.generation.attempt == 2).unwrap();
    assert_eq!(first_of_top.chunk_index, 0);
    let replacement: String = deltas
        .iter()
        .filter(|d| d.generation.attempt == 2)
        .map(|d| d.text.as_str())
        .collect();
    assert_eq!(replacement, "replacement text");
    // The committed final carries the higher attempt and arrives after every delta.
    let position = |pred: &dyn Fn(&FeedItem) -> bool| items.iter().rposition(pred).unwrap();
    let last_delta = position(&|i| matches!(i, FeedItem::Delta(_)));
    let final_at = position(
        &|i| matches!(i, FeedItem::Committed(e) if matches!(e.content, EnvelopePayload::MessageFinal(_))),
    );
    assert!(final_at > last_delta);
    let FeedItem::Committed(final_event) = &items[final_at] else {
        unreachable!()
    };
    let EnvelopePayload::MessageFinal(message) = &final_event.content else {
        unreachable!()
    };
    assert_eq!(message.completed.generation.attempt, 2);
    assert_eq!(
        message.completed.generation.message_id,
        deltas[0].generation.message_id
    );
    assert_feed_matches_history(
        &committed(&items),
        &room.page_events(None, 500).await.unwrap(),
    );
    room.shutdown().await.unwrap();
}

#[derive(Clone)]
struct SlowFirstRound;
impl Respond for SlowFirstRound {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let messages = body["messages"].to_string();
        if body["model"] == "fixture/judge" {
            if messages.contains("Score round 1") {
                delayed_round_score(
                    serde_json::json!({"scores":{"member0":{"score":8,"reason":"Clear"},"member1":{"score":6,"reason":"Thin"}}}),
                    Duration::from_millis(700),
                )
            } else if messages.contains("Score round ") {
                tied_round_score("member0", "member1")
            } else {
                let scores=["member0","member1"].map(|nick|serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|serde_json::json!({"criterionId":id,"score":5,"reasoning":"Supported"})))}));
                structured_judge(
                    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Tie breaker","whatToStealFromLoser":[],"tallyOverrideReason":null}),
                )
            }
        } else {
            streaming("A concrete proposal", "stop")
        }
    }
}

#[tokio::test]
async fn late_round_scores_keep_recipients_round_identity_payload_and_sequence_order() {
    let f = live::Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(SlowFirstRound)
        .with_priority(10)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    room.submit(f.request("@member0 @member1 $debate $ROUNDS=2 @judge Compare designs"))
        .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='verdict'") == 1)
        .await;
    let items = drain_settled(&mut feed, &room).await;
    let feed_events = committed(&items);
    let history = room.page_events(None, 500).await.unwrap();
    let scores: Vec<&CommittedEnvelope> = feed_events
        .iter()
        .filter(|e| matches!(e.content, EnvelopePayload::DebateRoundScore(_)))
        .collect();
    assert_eq!(scores.len(), 2, "one score per round");
    let rounds: Vec<u32> = scores
        .iter()
        .map(|e| match &e.content {
            EnvelopePayload::DebateRoundScore(s) => s.round,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        rounds,
        vec![1, 2],
        "scores keep round identity and commit order"
    );
    for event in &scores {
        assert_eq!(
            event.base.recipients,
            vec![Recipient::Me],
            "scores go to the human only"
        );
        assert_eq!(event.base.sender, Actor::Judge);
        // The payload delivered live is the payload recorded durably.
        assert!(history.contains(event));
    }
    assert_feed_matches_history(&feed_events, &history);
    room.shutdown().await.unwrap();
}

#[derive(Clone)]
struct TournamentAnswers;
impl Respond for TournamentAnswers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["model"] == "fixture/judge"
            && body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("summoned judge")
        {
            let user = body["messages"][1]["content"].as_str().unwrap();
            let data = user
                .split("Tournament prompt and anonymized answers, supplied as evidence:\n")
                .nth(1)
                .unwrap()
                .trim_start();
            let mut de = serde_json::Deserializer::from_str(data);
            let data: serde_json::Value = serde::Deserialize::deserialize(&mut de).unwrap();
            structured_judge(
                serde_json::json!({"ranking":data["entries"].as_array().unwrap().iter().map(|e|serde_json::json!({"label":e["label"],"score":8,"critique":"Supported proposal"})).collect::<Vec<_>>()}),
            )
        } else {
            streaming("A tournament answer.", "stop")
        }
    }
}

#[tokio::test]
async fn tournament_and_pick_events_arrive_in_commit_order_without_blind_judge_maps() {
    let f = live::Fixture::with_roster(3).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(TournamentAnswers)
        .with_priority(10)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    room.submit(f.request("@member0 @member1 @member2 $tournament Design a bridge"))
        .await;
    f.wait(|| {
        f.scalar("SELECT COUNT(*) FROM contests WHERE kind='tournament' AND json_extract(state_json,'$.state')='COMPLETE'")
            == 1
    })
    .await;
    // A question-first contest then adds collecting and pick events to the same feed.
    room.submit(f.request("@all $debate $ROUNDS=2 Compare designs?"))
        .await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM events WHERE kind='contest.pick_requested'") == 1)
        .await;
    let items = drain_settled(&mut feed, &room).await;
    let feed_events = committed(&items);
    let kinds: Vec<String> = feed_events
        .iter()
        .map(|e| {
            serde_json::to_value(&e.content).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    for needed in [
        "tournament.setup",
        "tournament.entry",
        "tournament.ranking",
        "tournament.champion",
        "contest.collecting",
        "contest.pick_requested",
    ] {
        assert!(
            kinds.iter().any(|k| k == needed),
            "{needed} missing from {kinds:?}"
        );
    }
    let champion = kinds
        .iter()
        .position(|k| k == "tournament.champion")
        .unwrap();
    let collecting = kinds
        .iter()
        .position(|k| k == "contest.collecting")
        .unwrap();
    assert!(
        champion < collecting,
        "feed keeps cross-contest commit order"
    );
    // Blind-judge label maps and private prompts are never part of anything delivered.
    let wire = serde_json::to_string(&items).unwrap();
    for secret in [
        "labelMap",
        "label_map",
        "tournament_labels",
        "PERSONA_",
        "member_prompts",
    ] {
        assert!(!wire.contains(secret), "{secret} leaked into the feed");
    }
    assert_feed_matches_history(&feed_events, &room.page_events(None, 500).await.unwrap());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn new_mode_events_and_lag_recovery_preserve_identities_and_order() {
    for text in [
        "@all $census Build a database service.",
        "@all $crosscheck Maintain service.\n--\nKeep service available.",
        "@all $survey Which checks help?",
        "@all $probe Intermittent noise, fan ruled out, inspect indicator only",
    ] {
        let root = tempfile::tempdir().unwrap();
        let launcher = Launcher::default();
        let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
        let mut feed = room.subscribe();
        let pending = room.submit(message4(text, EventId::new())).await;
        let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } = pending
        else {
            panic!("{text}: {pending:?}")
        };
        accepted(
            room.submit_commit_only(Request::ContestConfirm {
                request_id: EventId::new(),
                contest_id: plan.contest_id,
                plan_id: plan.plan_id,
            })
            .await,
        );
        let delivered = committed(&drain(&mut feed));
        assert!(!delivered.is_empty(), "{text}");
        // Step and artifact identities stay attached to the contest they belong to.
        let wire = serde_json::to_string(&delivered).unwrap();
        assert!(wire.contains(&plan.contest_id.to_string()), "{text}");
        let history = room.page_events(None, 500).await.unwrap();
        assert_feed_matches_history(&delivered, &history);
        // After a lag overflow the consumer recovers every missed commit from durable history.
        let seen = delivered.last().unwrap().commit.seq;
        for _ in 0..constants::FEED_CAPACITY + 1 {
            accepted(room.submit(set(4)).await);
        }
        assert!(matches!(
            feed.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
        ));
        let mut cursor = seen;
        let mut recovered = 0;
        loop {
            let page = room.events_after(Some(cursor), 500).await.unwrap();
            if page.is_empty() {
                break;
            }
            for event in page {
                assert!(event.commit.seq > cursor);
                cursor = event.commit.seq;
                recovered += 1;
            }
        }
        assert!(recovered >= constants::FEED_CAPACITY, "{text}");
        // Human feed access is not an agent-history capability: no member's history carries the
        // public preflight, plan identity or estimate that the human feed delivered.
        let leaked: i64 = store(&launcher)
            .call({
                let plan_id = plan.plan_id.to_string();
                move |db| {
                    Ok(db.query_row(
                        "SELECT COUNT(*) FROM history WHERE instr(content,?1)>0 OR instr(content,'estimatedUsd')>0 OR instr(content,'reservedUsd')>0",
                        [plan_id],
                        |r| r.get(0),
                    )?)
                }
            })
            .unwrap();
        assert_eq!(leaked, 0, "{text}");
        room.shutdown().await.unwrap();
    }
}
