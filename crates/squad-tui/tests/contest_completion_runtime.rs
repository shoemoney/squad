#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;

use fake_openrouter::*;
use serde_json::{Value, json};
use squad_core::room::RoomHandle;
use squad_protocol::*;
use squad_tui::{
    composed::Composed,
    fx::{ColorTier, FxQueue, Palette},
    room_adapter::{history, snapshot},
    session::{RoomPort, Session},
};
use std::{collections::BTreeMap, time::Duration};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

struct Answers;
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"][1]["content"].as_str().unwrap();
        let input = user
            .char_indices()
            .filter(|(_, c)| *c == '{')
            .find_map(|(i, _)| {
                serde_json::Deserializer::from_str(&user[i..])
                    .into_iter::<Value>()
                    .next()
                    .and_then(Result::ok)
                    .filter(|v| {
                        v.get("step").is_some()
                            || v.get("vectors").is_some()
                            || v.get("lens").is_some()
                    })
            });
        let Some(input) = input else {
            return streaming("BASELINE output", "stop");
        };
        let output = if input.get("step").is_some() {
            match input["step"].as_str().unwrap() {
                "cases" => {
                    json!({"cases":[{"localId":"a","kind":"typical","input":"Explain a queue","expect":["Useful answer"],"mustNot":[]},{"localId":"b","kind":"edge","input":"Explain overload","expect":["Concise answer"],"mustNot":[]}],"criteria":[{"localId":"r","name":"Correctness","description":"Address the question"},{"localId":"s","name":"Conciseness","description":"Avoid padding"}]})
                }
                "freeze" => {
                    let cases: Vec<_> = input["proposals"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .take(3)
                        .flat_map(|p| {
                            [
                                format!("{}:a", p["label"].as_str().unwrap()),
                                format!("{}:b", p["label"].as_str().unwrap()),
                            ]
                        })
                        .collect();
                    let dropped: Vec<_> = input["proposals"].as_array().unwrap().iter().skip(3).flat_map(|p| [json!({"caseRef":format!("{}:a",p["label"].as_str().unwrap()),"why":"duplicate"}),json!({"caseRef":format!("{}:b",p["label"].as_str().unwrap()),"why":"duplicate"})]).collect();
                    json!({"cases":cases,"dropped":dropped,"rubric":[{"name":"Correctness","description":"Address the question","sourceRefs":["P1:r"]},{"name":"Conciseness","description":"Avoid padding","sourceRefs":["P1:s"]}]})
                }
                "score" => {
                    json!({"cases":input["cases"].as_array().unwrap().iter().map(|c| json!({"caseLabel":c["caseLabel"],"outputs":c["outputs"].as_array().unwrap().iter().map(|o| json!({"label":o["label"],"scores":[{"criterion":"Correctness","score":4},{"criterion":"Conciseness","score":4}],"note":"Fully satisfies requested behavior"})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
                }
                stage => panic!("unexpected Tune stage {stage}"),
            }
        } else if input.get("lens").is_some() {
            let vector = |id: &str, diversity: bool| json!({"localId":id,"title":"Unbounded access","targetQuote":"All requests are accepted without authentication.","exploit":"An outsider submits arbitrary requests.","severity":"high","likelihood":"likely","blastRadius":"All stored data","diversity":diversity});
            json!({"vectors":[vector("a",false),vector("b",true)]})
        } else if input.get("tallies").is_some() {
            json!({"findings":[{"findingId":"f1","title":"Unauthenticated arbitrary requests","sourceVectorIds":input["vectors"].as_array().unwrap().iter().map(|v|v["vectorId"].clone()).collect::<Vec<_>>(),"severity":"high","likelihood":"likely","exploit":"Any outsider submits requests.","blastRadius":"All stored data","mitigation":"Authenticate and authorize each request.","rebuttalStatus":"survived","residual":"Credentials may still be abused.","challengeRejection":null}],"killedNotes":[]})
        } else {
            json!({"verdicts":input["vectors"].as_array().unwrap().iter().map(|v|json!({"vectorId":v["vectorId"],"verdict":"holds","reason":"Authentication is explicitly absent.","duplicateOf":null,"defeatingQuote":null})).collect::<Vec<_>>(),"additions":[]})
        };
        streaming(&output.to_string(), "stop")
    }
}

async fn completes_in_timeline(kind: ContestKind, command: &str, summary: &str) {
    let dir = tempfile::tempdir().unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: Some(dir.path().join("data")),
        working_dir: dir.path().into(),
        config_dir: None,
        default_data_dir: dir.path().join("data"),
    };
    let mut config = SquadConfig::load_with_env(
        &options,
        &BTreeMap::from([("OPENROUTER_API_KEY".into(), "fake-key".into())]),
    )
    .unwrap();
    config.agents = (0..6)
        .map(|i| config::AgentConfig {
            nick: format!("member{i}").parse().unwrap(),
            model: format!("fixture/member{i}"),
            persona: format!("PERSONA_{i}"),
            reasoning_effort: None,
        })
        .collect();
    config.judge.model = "fixture/judge".into();
    config.budget.daily_max_usd = 150.;
    config.budget.debate_confirm_usd = 0.;
    let fake = FakeOpenRouter::start().await;
    let models = config.agents.iter().map(|a| a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id| json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
    fake.catalog(json!({"data":models})).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(Answers)
        .mount(&fake.server)
        .await;
    let room = RoomHandle::open_with_client(
        config,
        "main",
        OpenMode::Interactive,
        squad_agents::openrouter::OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap(),
    )
    .await
    .unwrap();
    let mut feed = RoomPort::subscribe(&room, None).unwrap();
    let mut session = Session::default();
    session.restore(snapshot(&room.state().await, history(&room).await.unwrap()));
    let meta = SubmissionMeta::default();
    let parsed = parse(command, &meta, &session.snapshot.context).unwrap();
    let result = room
        .submit(
            Request::from_parsed_with_context(
                EventId::new(),
                parsed,
                &meta,
                &session.snapshot.context,
                true,
            )
            .unwrap(),
        )
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    let mut queue = FxQueue::new();
    let mut composed = Composed::default();
    let palette = Palette::new(ColorTier::TrueColor);
    let mut saw_active = false;
    let id = tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let delivery = feed.recv().await.unwrap();
            let terminal = match &delivery.item {
                FeedItem::Committed(e) => match &e.content {
                    EnvelopePayload::TuneState(s) if s.state == tune::TuneStatus::Complete => {
                        Some(s.contest_id)
                    }
                    EnvelopePayload::BlowItUpState(s)
                        if s.state == blow_it_up::BlowItUpStatus::Complete =>
                    {
                        Some(s.contest_id)
                    }
                    _ => None,
                },
                _ => None,
            };
            session.reduce(delivery.item, delivery.origin, Duration::ZERO, &mut queue);
            saw_active |= session.active_contest();
            // Exercise the same cached composition as the running room on every delivery.
            composed.prepare(&session, 120, &palette);
            if let Some(id) = terminal {
                break id;
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "contest did not reach COMPLETE: {:?}; error: {:?}",
            session.contests, session.last_error
        )
    });
    assert!(room.state().await.active_contest.is_none());
    assert!(
        room.export(
            if kind == ContestKind::Tune {
                ExportScope::Tune
            } else {
                ExportScope::BlowItUp
            },
            Some(id)
        )
        .await
        .unwrap()
        .contains(summary)
    );
    let events = history(&room).await.unwrap();
    assert!(
        events.iter().any(
            |e| matches!(&e.content, EnvelopePayload::CaseOpened(c) if c.contest_id == Some(id))
        ),
        "fixture must exercise internal judge cases"
    );
    assert_timeline(&session, &mut composed, id, summary, &palette);
    assert!(saw_active, "contest never appeared active");
    let mut restored = Session::default();
    restored.restore(snapshot(&room.state().await, events));
    assert_timeline(&restored, &mut Composed::default(), id, summary, &palette);
    room.shutdown().await.unwrap();
}

fn assert_timeline(
    session: &Session,
    composed: &mut Composed,
    id: ContestId,
    summary: &str,
    palette: &Palette,
) {
    composed.prepare(session, 120, palette);
    let card = composed
        .transcript
        .message(id.to_string().parse().unwrap())
        .expect("contest timeline card");
    let text = format!(
        "{}\n{}",
        card.source,
        card.widget_rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(text.contains(summary), "missing report: {text}");
    assert!(!session.active_contest(), "COMPLETE contest still active");
    assert!(
        session
            .cases
            .values()
            .all(|c| c.opened.contest_id != Some(id)),
        "internal judge case still awaiting ruling"
    );
}

#[tokio::test]
async fn tune_complete_clears_judging_and_renders_report() {
    completes_in_timeline(
        ContestKind::Tune,
        "$TUNE_GENERATIONS=1 @all $tune PROMPT answer clearly\n--\nprovide helpful concise answers",
        "Nothing to fix",
    )
    .await;
}

#[tokio::test]
async fn blow_it_up_complete_clears_judging_and_renders_report() {
    completes_in_timeline(
        ContestKind::BlowItUp,
        "@all $blow_it_up ```All requests are accepted without authentication.```",
        "Unauthenticated arbitrary requests",
    )
    .await;
}
