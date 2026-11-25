#![allow(dead_code)]
use serde::Serialize;
use serde_json::json;
use squad_cli::room_api::{Progress, RoomApi, RoomError};
use squad_protocol::{parser::ParseContext, *};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use tokio::sync::broadcast;

pub const ID: &str = "01900000-0000-7000-8000-000000000001";
pub fn event(n: u32, kind: &str, payload: serde_json::Value) -> CommittedEnvelope {
    serde_json::from_value(json!({"version":1,"id":format!("01900000-0000-7000-8000-{n:012x}"),"roomId":"main","correlationId":ID,"sender":if kind == "message.final" {"alpha"} else {"system"},"recipients":["me"],"createdAt":"2026-11-24T00:00:00Z","seq":n.to_string(),"acceptedAt":"2026-11-24T00:00:00Z","commitIndex":0,"commitSize":1,"kind":kind,"payload":payload})).unwrap()
}
pub fn reply(n: u32, content: &str) -> CommittedEnvelope {
    event(
        n,
        "message.final",
        json!({"messageId":ID,"workId":ID,"triggerEventId":ID,"attempt":1,"model":"public/model","finishReason":"stop","usage":null,"inReplyToMessageId":ID,"content":content}),
    )
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub room: &'static str,
    #[serde(skip)]
    pub context: ParseContext,
    #[serde(skip)]
    pub progress: Progress,
}
pub struct FakeRoom {
    pub requests: RefCell<Vec<Request>>,
    pub results: RefCell<VecDeque<RequestResult>>,
    pub events: RefCell<Vec<CommittedEnvelope>>,
    pub emitted: RefCell<Vec<CommittedEnvelope>>,
    pub shutdowns: std::rc::Rc<Cell<u32>>,
    pub context: ParseContext,
    pub progress: Cell<Progress>,
    pub fail_shutdown: Cell<bool>,
    pub feed: broadcast::Sender<FeedItem>,
}
impl Default for FakeRoom {
    fn default() -> Self {
        Self {
            requests: RefCell::default(),
            results: RefCell::default(),
            events: RefCell::default(),
            emitted: RefCell::default(),
            shutdowns: std::rc::Rc::new(Cell::new(0)),
            context: ParseContext {
                roster: vec!["alpha".parse().unwrap(), "beta".parse().unwrap()],
                ..Default::default()
            },
            progress: Cell::new(Progress::Succeeded),
            fail_shutdown: Cell::new(false),
            feed: broadcast::channel(1).0,
        }
    }
}
impl RoomApi for FakeRoom {
    type State = State;
    async fn submit(&self, request: Request) -> RequestResult {
        self.requests.borrow_mut().push(request);
        self.events
            .borrow_mut()
            .append(&mut self.emitted.borrow_mut());
        self.results
            .borrow_mut()
            .pop_front()
            .unwrap_or(RequestResult::Accepted {
                request_id: ID.parse().unwrap(),
                event_ids: vec![ID.parse().unwrap()],
                cursor: Some(Cursor::new(1).unwrap()),
            })
    }
    fn subscribe(&self) -> broadcast::Receiver<FeedItem> {
        self.feed.subscribe()
    }
    async fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> Result<Vec<CommittedEnvelope>, RoomError> {
        let mut events: Vec<_> = self
            .events
            .borrow()
            .iter()
            .filter(|event| before.is_none_or(|cursor| event.commit.seq < cursor))
            .cloned()
            .collect();
        events.sort_by_key(|event| event.commit.seq);
        Ok(events
            .into_iter()
            .rev()
            .take(limit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect())
    }
    async fn state(&self) -> State {
        State {
            room: "main",
            context: self.context.clone(),
            progress: self.progress.get(),
        }
    }
    async fn export(&self, _: ExportScope, _: Option<ContestId>) -> Result<String, RoomError> {
        Ok("# Export\n".into())
    }
    async fn shutdown(&self) -> Result<(), RoomError> {
        self.shutdowns.set(self.shutdowns.get() + 1);
        if self.fail_shutdown.get() {
            Err(RoomError {
                code: ErrorCode::NOT_READY,
                message: "Shutdown failed.".into(),
                retryable: false,
            })
        } else {
            Ok(())
        }
    }
    fn parse_context(state: &State) -> ParseContext {
        state.context.clone()
    }
    fn progress(state: &State, _: EventId) -> Progress {
        state.progress
    }
    fn relevant(_: &State, _: EventId, event: &CommittedEnvelope) -> bool {
        event.base.correlation_id == ID.parse().unwrap()
    }
}
pub fn args(message: &str, json: bool, no_wait: bool) -> squad_cli::args::Send {
    squad_cli::args::Send {
        message: message.into(),
        quote: vec![],
        yes: false,
        json,
        no_wait,
    }
}
pub async fn send(
    room: &FakeRoom,
    args: &squad_cli::args::Send,
    confirm: Option<bool>,
) -> (u8, String, String) {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = squad_cli::send::run(
        room,
        args,
        &mut squad_cli::output::Output {
            json: args.json,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
        || confirm,
    )
    .await
    .unwrap();
    (
        code,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}
pub fn normalize(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
            if value.get("requestId").is_some() {
                value["requestId"] = json!(ID);
            }
            serde_json::to_string(&value).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn preflight(kind: ContestKind) -> PendingPreflight {
    PendingPreflight::Contest(ContestPreflight {
        contest_id: ID.parse().unwrap(),
        kind,
        plan_id: ID.parse().unwrap(),
        expires_at: "2026-11-24T00:10:00Z".parse().unwrap(),
        estimate: DebateEstimate {
            estimated_usd: 2.5,
            reserved_usd: 2.5,
            remaining_usd: 10.,
            model_calls: 12,
        },
        participants: vec!["alpha".parse().unwrap(), "beta".parse().unwrap()],
        confirmation_required: true,
        warning: Some(Text::new("beta sits out the final step").unwrap()),
        settings: json!({"reusedInputs":[ID],"passes":3}),
    })
}

pub fn database(root: &std::path::Path, events: Vec<CommittedEnvelope>) -> rusqlite::Connection {
    let mut db = squad_store::open(root.join("squad.db")).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    db.execute("INSERT INTO room_meta VALUES ('main',1,1,1,1,NULL)", [])
        .unwrap();
    let request_id = EventId::new();
    squad_store::commit::commit(&mut db, request_id, "fixture", "fixture", |tx, group| {
        for event in events {
            group.push(
                tx,
                ProducerEnvelope {
                    base: event.base,
                    content: event.content,
                },
            )?;
        }
        Ok(group.accepted(request_id))
    })
    .unwrap();
    db
}
