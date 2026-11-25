#![allow(dead_code)]
use squad_agents::{openrouter::OpenRouterClient, resolver::ResolutionSnapshot};
use squad_core::{bus::InProcessBus, room::*};
use squad_protocol::{
    config::{LoadOptions, SquadConfig},
    *,
};
use squad_store::Store;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};
#[derive(Default, Clone)]
pub struct Launcher {
    pub store: Arc<Mutex<Option<Store>>>,
    pub wakes: Arc<Mutex<Vec<WorkId>>>,
    pub modes: Arc<Mutex<Vec<OpenMode>>>,
    pub fail_ready: bool,
    pub stopped: Arc<Mutex<bool>>,
}
struct Runtime(Launcher);
impl RoomRuntime for Runtime {
    fn ready(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async {
            if self.0.fail_ready {
                Err(RoomError::new(
                    ErrorCode::NOT_READY,
                    "fixture readiness failed",
                ))
            } else {
                Ok(())
            }
        })
    }
    fn wake(&self, ids: &[WorkId]) {
        self.0.wakes.lock().unwrap().extend_from_slice(ids);
    }
    fn shutdown(self: Box<Self>) -> RoomFuture<'static, ()> {
        Box::pin(async move {
            *self.0.stopped.lock().unwrap() = true;
            self.0.store.lock().unwrap().take();
            Ok(())
        })
    }
}
impl RuntimeLauncher for Launcher {
    fn recover_on_open_mode(&self, _: &Store, _: OpenMode, _: DateTime) -> Result<(), RoomError> {
        Ok(())
    }
    fn start_with_mode(
        &self,
        _: &SquadConfig,
        store: Store,
        _: InProcessBus,
        _: OpenRouterClient,
        _: ResolutionSnapshot,
        mode: OpenMode,
    ) -> Result<Box<dyn RoomRuntime>, RoomError> {
        *self.store.lock().unwrap() = Some(store);
        self.modes.lock().unwrap().push(mode);
        Ok(Box::new(Runtime(self.clone())))
    }
}
pub fn config(root: &Path) -> SquadConfig {
    SquadConfig::load_with_env(
        &LoadOptions {
            env_file: None,
            data_dir: Some(root.join("data")),
            working_dir: root.to_owned(),
            config_dir: None,
            default_data_dir: root.join("data"),
        },
        &BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "fake-room-key".into()),
            ("SQUAD_MODEL_A".into(), "test/model".into()),
            ("SQUAD_MODEL_B".into(), "test/model".into()),
            ("SQUAD_JUDGE_MODEL".into(), "test/model".into()),
        ]),
    )
    .unwrap()
}
pub async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/models")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":"test/model","context_length":1000000,"pricing":{"prompt":"0.000001","completion":"0.000002"}}]}))).mount(&server).await;
    server
}
pub fn client(server: &MockServer) -> OpenRouterClient {
    OpenRouterClient::new(&format!("{}/", server.uri()), "fake-room-key").unwrap()
}
pub async fn open(root: &Path, launcher: &Launcher, mode: OpenMode) -> (RoomHandle, MockServer) {
    let server = server().await;
    let handle =
        RoomHandle::open_with_launcher(config(root), "main", mode, client(&server), launcher)
            .await
            .unwrap();
    (handle, server)
}
pub fn store(launcher: &Launcher) -> Store {
    launcher.store.lock().unwrap().as_ref().unwrap().clone()
}
pub fn nick(name: &str) -> MemberNick {
    name.parse().unwrap()
}
pub fn message(text: &str, id: EventId) -> Request {
    let context = parser::ParseContext {
        roster: vec![nick("a"), nick("b")],
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    Request::from_parsed(id, parse(text, &meta, &context).unwrap(), &meta, false).unwrap()
}
pub fn set(value: u8) -> Request {
    Request::SetVars {
        request_id: EventId::new(),
        updates: vec![VariableUpdate {
            name: VariableName::Rounds,
            value,
        }],
    }
}
pub fn accepted(result: RequestResult) -> Cursor {
    match result {
        RequestResult::Accepted {
            cursor: Some(cursor),
            ..
        } => cursor,
        other => panic!("{other:?}"),
    }
}
/// Four-member roster `m0..m3` for the four new modes, which need three or more participants.
pub fn config4(root: &Path) -> SquadConfig {
    let mut cfg = config(root);
    cfg.agents = (0..4)
        .map(|i| squad_protocol::config::AgentConfig {
            nick: nick(&format!("m{i}")),
            model: "test/model".into(),
            persona: format!("PERSONA_{i}"),
            reasoning_effort: None,
        })
        .collect();
    cfg.budget.daily_max_usd = 200.;
    cfg.budget.debate_confirm_usd = 0.;
    cfg
}
pub async fn open4(root: &Path, launcher: &Launcher, mode: OpenMode) -> (RoomHandle, MockServer) {
    let server = server().await;
    let handle =
        RoomHandle::open_with_launcher(config4(root), "main", mode, client(&server), launcher)
            .await
            .unwrap();
    (handle, server)
}
pub fn message4(text: &str, id: EventId) -> Request {
    let context = parser::ParseContext {
        roster: (0..4).map(|i| nick(&format!("m{i}"))).collect(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    Request::from_parsed(id, parse(text, &meta, &context).unwrap(), &meta, false).unwrap()
}

// ---- Committed-event fixtures shared by the history, feed, state and clear tests ----

pub const CONTEST: &str = "01900000-0000-7000-8000-000000000100";
pub const CASE: &str = "01900000-0000-7000-8000-000000000300";
pub fn uuid(n: u32) -> String {
    format!("01900000-0000-7000-8000-{n:012x}")
}
pub fn payload(kind: &str, body: serde_json::Value) -> EnvelopePayload {
    serde_json::from_value(serde_json::json!({"kind":kind,"payload":body})).unwrap()
}
/// A fixture event: who sent it, who may see it, and its owning payload.
pub type Fixture = (Actor, Vec<Recipient>, EnvelopePayload);
pub fn system(kind: &str, body: serde_json::Value) -> Fixture {
    (Actor::System, vec![Recipient::Me], payload(kind, body))
}
pub fn judge(kind: &str, body: serde_json::Value) -> Fixture {
    (Actor::Judge, vec![Recipient::Me], payload(kind, body))
}
/// Commit every fixture as its own receipt so paging can split between them.
pub fn commit_each(launcher: &Launcher, items: Vec<Fixture>) -> Vec<CommittedEnvelope> {
    let store = store(launcher);
    let mut out = Vec::new();
    for (sender, recipients, content) in items {
        let request = EventId::new();
        let outcome = store
            .commit(
                request,
                "fixture".into(),
                format!("fixture-{request}"),
                move |tx, group| {
                    group.push(
                        tx,
                        squad_core::router::envelope(request, sender, recipients, content),
                    )?;
                    Ok(group.accepted(request))
                },
            )
            .unwrap();
        out.extend(outcome.events);
    }
    out
}
pub fn debate_state(status: &str, round: u32, n: u64) -> Fixture {
    system(
        "debate.state",
        serde_json::json!({"debateId":CONTEST,"state":status,"debaters":["a","b"],"round":round,"totalRounds":3,"judgeModel":"judge/model","judgeRequested":true,"stateVersion":n,"terminalReason":null,"caseId":CASE,"caseIds":[CASE]}),
    )
}
fn generation() -> serde_json::Value {
    serde_json::json!({"messageId":uuid(90),"workId":uuid(91),"triggerEventId":uuid(92),"attempt":1,"model":"public/model","finishReason":"stop","usage":null})
}
/// A judged debate whose round 3 score arrives late as an unscored marker, ending in a tally-bearing verdict.
pub fn debate_fixture() -> Vec<Fixture> {
    use serde_json::json;
    let scores = ["a", "b"].map(|nick| {
        let score = if nick == "a" { 5 } else { 3 };
        json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|json!({"criterionId":id,"score":score,"reasoning":"Supported by the evidence"})))})
    });
    let judged: squad_protocol::JudgeVerdict = serde_json::from_value(json!({"winner":"a","scores":scores,"reasoning":"a wins on the evidence.","whatToStealFromLoser":[],"tallyOverrideReason":null})).unwrap();
    let tally = BTreeMap::from([(nick("a"), 2), (nick("b"), 1)]);
    let assembled =
        squad_protocol::verdict::assemble_verdict(judged, &[nick("a"), nick("b")], &tally).unwrap();
    let mut verdict = generation();
    verdict.as_object_mut().unwrap().extend(
        json!({"debateId":CONTEST,"caseId":CASE,"judgeInvocationId":uuid(93),"rubricVersion":"v1"})
            .as_object()
            .unwrap()
            .clone(),
    );
    verdict.as_object_mut().unwrap().extend(
        serde_json::to_value(&assembled)
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
    );
    vec![
        system(
            "debate.setup",
            json!({"debateId":CONTEST,"topicMessageId":uuid(70),"topicText":"Build a search engine","solutions":[{"nick":"a","messageId":uuid(71),"text":"Use an index"},{"nick":"b","messageId":uuid(72),"text":"Scan everything"}],"instruction":"instruction","rounds":3,"judgeModel":"judge/model","judgeSwapped":false}),
        ),
        debate_state("ROUND_RUNNING", 1, 1),
        (
            Actor::Member(nick("a")),
            vec![Recipient::Me],
            payload(
                "debate.turn",
                json!({"messageId":uuid(50),"workId":uuid(51),"triggerEventId":uuid(52),"attempt":1,"debateId":CONTEST,"round":1,"speaker":"a","outcome":{"status":"completed","content":"Index the pages.","model":"public/model","finishReason":"stop","usage":null}}),
            ),
        ),
        debate_state("ROUND_RUNNING", 3, 2),
        judge(
            "debate.round_score",
            json!({"debateId":CONTEST,"caseId":CASE,"round":1,"scores":{"a":{"score":9,"reason":"Clear"},"b":{"score":7,"reason":"Slow"}},"tally":{"a":1,"b":0}}),
        ),
        judge(
            "debate.round_score",
            json!({"debateId":CONTEST,"caseId":CASE,"round":3,"scores":null,"tally":{"a":1,"b":0},"reason":"unscored"}),
        ),
        system(
            "debate.turn",
            json!({"messageId":uuid(53),"workId":uuid(54),"triggerEventId":uuid(55),"attempt":1,"debateId":CONTEST,"round":3,"speaker":"b","outcome":{"status":"forfeit","reasonCode":"timeout","text":"Timed out"}}),
        ),
        debate_state("JUDGING", 3, 3),
        judge("verdict", verdict),
        debate_state("COMPLETED", 3, 4),
    ]
}
fn generic_state(kind: &str, status: &str, version: u64, id: &str) -> Fixture {
    system(
        "contest.state",
        serde_json::json!({"kind":"generic","state":{"contestId":id,"kind":kind,"stateVersion":version,"status":status,"artifactEventIds":[],"terminalReason":null}}),
    )
}
fn artifact(id: &str, step: u32, kind: &str, data: serde_json::Value) -> Fixture {
    system(
        "contest.artifact",
        serde_json::json!({"contestId":id,"stepId":uuid(step),"artifactKind":kind,"sourceRefs":[],"data":data}),
    )
}
/// Tournament and grill public artifacts plus question-first collection and pick events.
pub fn tournament_grill_pick_fixture() -> Vec<Fixture> {
    use serde_json::json;
    let tournament = uuid(500);
    let grill = uuid(600);
    let pick = uuid(700);
    vec![
        generic_state("tournament", "RUNNING", 1, &tournament),
        artifact(
            &tournament,
            80,
            "public",
            json!({"round":1,"entries":[["a","entry"]],"champion":null}),
        ),
        generic_state("tournament", "COMPLETED", 2, &tournament),
        generic_state("grill", "RUNNING", 1, &grill),
        artifact(
            &grill,
            81,
            "public",
            json!({"questions":[{"round":1,"griller":"b","question":"Cost?","answer":"Cache"}]}),
        ),
        system(
            "contest.collecting",
            json!({"contestId":pick,"kind":"grill","questionMessageId":uuid(710),"question":"Plan the launch?","addressed":["a","b"]}),
        ),
        system(
            "contest.pick_requested",
            json!({"contestId":pick,"kind":"grill","questionMessageId":uuid(710),"eligible":["a","b"],"requiredPicks":1,"banner":"PICK"}),
        ),
        system(
            "contest.picked",
            json!({"contestId":pick,"kind":"grill","nicks":["a"],"sourceMessageIds":[uuid(711)]}),
        ),
    ]
}
/// Setup, state, artifact and report-like events for census, crosscheck, survey and probe, with skipped members.
pub fn new_modes_fixture() -> Vec<Fixture> {
    use serde_json::json;
    let mut items = Vec::new();
    for (index, kind) in ["census", "crosscheck", "survey", "probe"]
        .into_iter()
        .enumerate()
    {
        let id = uuid(800 + index as u32);
        items.push(generic_state(kind, "RUNNING", 1, &id));
        items.push(artifact(
            &id,
            90 + index as u32,
            "public",
            json!({"label":format!("{kind} setup"),"skippedMembers":["d"],"humanVisible":{"estimate":"2-4"},"partial":true}),
        ));
        items.push(system(
            "contest.report",
            json!({"contestId":id,"stepId":uuid(90 + index as u32),"artifactEventIds":[],"skippedWorkIds":[uuid(95)],"data":{"kind":kind,"skippedMembers":["d"]}}),
        ));
        items.push(generic_state(kind, "STOPPED", 2, &id));
    }
    items
}
pub fn page_all(
    room: &RoomHandle,
    limit: usize,
) -> impl std::future::Future<Output = Vec<CommittedEnvelope>> + '_ {
    async move {
        let mut out = Vec::new();
        let mut before = None;
        loop {
            let page = room.page_events(before, limit).await.unwrap();
            if page.is_empty() {
                break;
            }
            before = Some(page[0].commit.seq);
            let mut merged = page;
            merged.extend(out);
            out = merged;
        }
        out
    }
}
