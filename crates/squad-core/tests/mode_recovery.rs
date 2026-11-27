//! M8.P11.L1.T3 / M9.P4.L1.T3: crash and recover each new mode at its assignment,
//! artifact, barrier, execution, human-step, save and next-work boundaries.
//!
//! Every mode's own workflow file (its fixture and scripted provider) is included
//! whole, so the provider script is exactly the one the mode's workflow tests use.
//! A gate in front of that script serves the first `k` provider calls and then leaves
//! every later request hanging. Once `k` calls are served the room is shut down mid
//! flight, reopened over the same data directory and run to its terminal state.
//!
//! Against a control run of the same mode the recovered run must: reach the same
//! terminal state; leave every immutable row written before the crash (events, frozen
//! private maps, step assignments, artifacts, frozen inputs, executions) byte for byte
//! unchanged, so no private draw is redrawn; keep every attempt that completed before
//! the crash exactly once; and complete the same multiset of work kinds with a
//! balanced ledger.
// Each included workflow file loads the shared fake provider module on its own.
#![allow(clippy::duplicate_mod)]
use squad_agents::openrouter::OpenRouterClient;
use squad_core::room::RoomHandle;
use squad_protocol::*;
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

type Local<T> = Pin<Box<dyn Future<Output = T>>>;

/// One mode's scripted environment, handed over by the mode's included harness.
pub struct Rig {
    pub config: SquadConfig,
    pub base_url: String,
    pub mount: Box<dyn Fn(Mock) -> Local<()>>,
    pub inner: Arc<dyn Respond>,
    pub starts: Vec<String>,
    /// Inputs sent, in order, each time the contest pauses for a human.
    pub human: Vec<Option<&'static str>>,
    pub _keep: Box<dyn Any>,
}

struct GateState {
    seen: AtomicUsize,
    served: AtomicUsize,
    limit: AtomicUsize,
}
struct Gate {
    inner: Arc<dyn Respond>,
    state: Arc<GateState>,
}
impl Respond for Gate {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        let n = self.state.seen.fetch_add(1, SeqCst);
        if n < self.state.limit.load(SeqCst) {
            self.state.served.fetch_add(1, SeqCst);
            self.inner.respond(request)
        } else {
            ResponseTemplate::new(200).set_delay(Duration::from_secs(20))
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Crash {
    /// After this many provider calls have been served.
    AfterCalls(usize),
    /// While paused for a human, after this many of the scripted inputs were sent.
    AtHuman(usize),
}

fn request(config: &SquadConfig, text: &str) -> Request {
    let ctx = parser::ParseContext {
        roster: config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    Request::from_parsed(
        EventId::new(),
        parse(text, &meta, &ctx).unwrap(),
        &meta,
        true,
    )
    .unwrap()
}

type Rows = BTreeSet<String>;
/// Tables whose rows are written once and never change.
const IMMUTABLE: &[(&str, &str)] = &[
    ("events", "SELECT seq,id,kind,payload FROM events"),
    (
        "contest_private_maps",
        "SELECT contest_id,scope_key,map_json FROM contest_private_maps",
    ),
    (
        "contest_assignments",
        "SELECT step_id,target,context_id,payload_refs_json,assignment_json FROM contest_assignments",
    ),
    ("contest_artifacts", "SELECT * FROM contest_artifacts"),
    ("mode_artifacts", "SELECT * FROM mode_artifacts"),
    ("forge_inputs", "SELECT * FROM forge_inputs"),
    ("negotiate_inputs", "SELECT * FROM negotiate_inputs"),
    ("forecast_work_inputs", "SELECT * FROM forecast_work_inputs"),
    ("tune_executions", "SELECT * FROM tune_executions"),
];

struct Snapshot {
    tables: BTreeMap<&'static str, Rows>,
    /// `work_id|attempt|outcome` for every attempt that completed.
    completed: Rows,
    completed_work: Rows,
}

fn db(config: &SquadConfig) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        config.data_dir.join("squad.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}

fn rows(db: &rusqlite::Connection, sql: &str) -> Rows {
    let mut query = db.prepare(sql).unwrap();
    let width = query.column_count();
    query
        .query_map([], |r| {
            Ok((0..width)
                .map(|i| match r.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join("|"))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn snapshot(config: &SquadConfig) -> Snapshot {
    let db = db(config);
    Snapshot {
        tables: IMMUTABLE
            .iter()
            .map(|(name, sql)| (*name, rows(&db, sql)))
            .collect(),
        completed: rows(
            &db,
            "SELECT work_id,attempt,outcome FROM attempts WHERE outcome='completed'",
        ),
        completed_work: rows(&db, "SELECT work_id FROM work WHERE status='completed'"),
    }
}

fn scalar(config: &SquadConfig, sql: &str) -> i64 {
    db(config).query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}

fn awaiting_human(config: &SquadConfig) -> Option<(ContestId, u64)> {
    db(config)
        .query_row(
            "SELECT contest_id,state_version FROM contests WHERE active=1 AND json_extract(state_json,'$.state')='AWAITING_HUMAN'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok()
        .map(|(id, v)| (id.parse().unwrap(), v as u64))
}

fn terminal(config: &SquadConfig) -> bool {
    scalar(config, "SELECT COUNT(*) FROM contests") > 0
        && scalar(config, "SELECT COUNT(*) FROM contests WHERE active=1") == 0
}

async fn open(rig: &Rig) -> RoomHandle {
    RoomHandle::open_with_client(
        rig.config.clone(),
        "main",
        OpenMode::Interactive,
        OpenRouterClient::new(&rig.base_url, "fake-key").unwrap(),
    )
    .await
    .unwrap()
}

async fn answer(rig: &Rig, room: &RoomHandle, text: Option<&str>) {
    let Some((contest_id, version)) = awaiting_human(&rig.config) else {
        return;
    };
    room.submit(Request::HumanAnswer {
        request_id: EventId::new(),
        contest_id,
        expected_state_version: version,
        text: text.map(str::to_owned),
    })
    .await;
}

async fn crash(room: RoomHandle) {
    // A real crash leaves no goodbye; the shutdown only stops this process's tasks.
    let _ = tokio::time::timeout(Duration::from_secs(8), room.shutdown()).await;
}

fn assert_ledger(config: &SquadConfig, label: &str) {
    let (spent, reserved, unknown) = {
        let db = db(config);
        db.query_row(
            "SELECT COALESCE(SUM(spent_micros),0),COALESCE(SUM(reserved_micros),0),COALESCE(SUM(unknown_calls),0) FROM spend_daily",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )
        .unwrap()
    };
    let actual = scalar(
        config,
        "SELECT COALESCE(SUM(reported_cost_micros),0) FROM attempts",
    );
    let held = scalar(
        config,
        "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
    );
    let pending = scalar(config, "SELECT COUNT(*) FROM attempts WHERE unresolved=1");
    assert_eq!(
        (spent, reserved, unknown),
        (actual, held, pending),
        "{label}: ledger"
    );
    // A call that was in flight at the crash never reported a cost or a provider id, so
    // the ledger keeps its worst-case hold and counts it unknown rather than guessing.
    // Every other reservation must be gone once the contest is terminal.
    assert!(
        held == 0 || (pending > 0 && pending <= 12),
        "{label}: {held} micros are still reserved with {pending} unresolved attempts"
    );
}

struct Outcome {
    /// Provider calls the gate served in total.
    served: usize,
    /// Completed work kinds, counted.
    kinds: BTreeMap<String, usize>,
    /// Work kinds in the order their first attempt started.
    order: Vec<String>,
    /// `(kind, state, reason)` of every contest at the end.
    terminal: Vec<String>,
    before: Option<Snapshot>,
    after: Snapshot,
    /// Completed attempts per work item at the end.
    completed_per_work: BTreeMap<String, i64>,
}

async fn execute(make: &impl Fn() -> Local<Rig>, crash_at: Option<Crash>, label: &str) -> Outcome {
    let rig = make().await;
    let state = Arc::new(GateState {
        seen: AtomicUsize::new(0),
        served: AtomicUsize::new(0),
        limit: AtomicUsize::new(match crash_at {
            Some(Crash::AfterCalls(k)) => k,
            _ => usize::MAX,
        }),
    });
    (rig.mount)(
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Gate {
                inner: rig.inner.clone(),
                state: state.clone(),
            })
            .with_priority(1),
    )
    .await;
    let mut room = open(&rig).await;
    for text in &rig.starts {
        let result = room.submit(request(&rig.config, text)).await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{label}: {text}: {result:?}"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut crash_at = crash_at;
    let mut before = None;
    let mut reached = None;
    let mut answered = BTreeSet::new();
    loop {
        assert!(Instant::now() < deadline, "{label}: no terminal state");
        tokio::time::sleep(Duration::from_millis(15)).await;
        let due = match crash_at {
            Some(Crash::AfterCalls(k)) => {
                let served = state.served.load(SeqCst);
                if served >= k {
                    let since = *reached.get_or_insert_with(Instant::now);
                    state.seen.load(SeqCst) > served || since.elapsed() > Duration::from_millis(500)
                } else {
                    false
                }
            }
            _ => false,
        };
        let mut human_crash = false;
        if let Some((contest, version)) = awaiting_human(&rig.config)
            && !answered.contains(&(contest, version))
        {
            let first = !answered.iter().any(|(c, _)| *c == contest);
            let stop_after = match crash_at {
                Some(Crash::AtHuman(n)) if first => Some(n),
                _ => None,
            };
            if stop_after == Some(0) {
                human_crash = true;
            } else {
                answered.insert((contest, version));
                for (i, text) in rig.human.iter().enumerate() {
                    answer(&rig, &room, *text).await;
                    if stop_after == Some(i + 1) {
                        human_crash = true;
                        break;
                    }
                }
            }
        }
        if due || human_crash {
            before = Some(snapshot(&rig.config));
            crash(room).await;
            state.limit.store(usize::MAX, SeqCst);
            crash_at = None;
            room = open(&rig).await;
            continue;
        }
        if terminal(&rig.config) {
            break;
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(8), room.shutdown()).await;
    assert_ledger(&rig.config, label);
    let db = db(&rig.config);
    let mut kinds = BTreeMap::new();
    for kind in db
        .prepare("SELECT kind FROM work WHERE status='completed'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
    {
        *kinds.entry(kind.unwrap()).or_insert(0) += 1;
    }
    let mut order = vec![];
    for kind in db
        .prepare("SELECT w.kind FROM attempts a JOIN work w ON w.work_id=a.work_id ORDER BY a.started_at,a.rowid")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
    {
        order.push(kind.unwrap());
    }
    let terminal = db
        .prepare("SELECT kind,json_extract(state_json,'$.state'),COALESCE(json_extract(state_json,'$.terminalReason'),'') FROM contests ORDER BY rowid")
        .unwrap()
        .query_map([], |r| {
            Ok(format!(
                "{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut completed_per_work = BTreeMap::new();
    for row in db
        .prepare("SELECT work_id,COUNT(*) FROM attempts WHERE outcome='completed' GROUP BY work_id")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
    {
        let (work, n) = row.unwrap();
        completed_per_work.insert(work, n);
    }
    Outcome {
        served: state.seen.load(SeqCst),
        kinds,
        order,
        terminal,
        before,
        after: snapshot(&rig.config),
        completed_per_work,
    }
}

/// Crash points drawn from the control run's own call order, at most `max` of them: before
/// the first call (assignment), after the first accepted call of every work kind
/// (artifact), just before the first call of every later kind (barrier and next work)
/// and just before the last call (save).
fn points(control: &Outcome, max: usize) -> Vec<usize> {
    let mut points = BTreeSet::from([0, 1, control.order.len().saturating_sub(1)]);
    let mut seen = BTreeSet::new();
    for (i, kind) in control.order.iter().enumerate() {
        if seen.insert(kind.clone()) {
            points.insert(i);
            points.insert(i + 1);
        }
    }
    let all: Vec<_> = points
        .into_iter()
        .filter(|p| *p < control.order.len())
        .collect();
    // Keep the sweep affordable: the first and last, the rest spread evenly.
    match max {
        0 => vec![],
        1 => all.into_iter().take(1).collect(),
        _ if all.len() <= max => all,
        _ => {
            let mut kept: BTreeSet<_> = (0..max)
                .map(|i| all[i * (all.len() - 1) / (max - 1)])
                .collect();
            kept.insert(all[0]);
            kept.insert(*all.last().unwrap());
            kept.into_iter().collect()
        }
    }
}

async fn sweep(mode: &str, make: impl Fn() -> Local<Rig>, human_points: usize, max_points: usize) {
    let control = execute(&make, None, &format!("{mode} control")).await;
    assert!(
        control.terminal.iter().all(|t| !t.contains("|failed|")),
        "{mode}: control run failed: {:?}",
        control.terminal
    );
    assert!(control.order.len() >= 5, "{mode}: {:?}", control.order);
    let mut plan: Vec<Crash> = points(&control, max_points)
        .into_iter()
        .map(Crash::AfterCalls)
        .collect();
    plan.extend((0..human_points).map(Crash::AtHuman));
    assert!(plan.len() >= 2, "{mode}: too few boundaries {plan:?}");
    for crash_at in plan {
        let label = format!("{mode} crash {crash_at:?}");
        let run = execute(&make, Some(crash_at), &label).await;
        let before = run
            .before
            .as_ref()
            .unwrap_or_else(|| panic!("{label}: the crash point was never reached"));
        assert_eq!(run.terminal, control.terminal, "{label}: different outcome");
        assert_eq!(
            run.kinds, control.kinds,
            "{label}: different completed work"
        );
        assert!(
            !before.tables["events"].is_empty(),
            "{label}: the snapshot saw no events"
        );
        if let Crash::AfterCalls(k) = crash_at
            && k >= 3
        {
            assert!(
                !before.completed_work.is_empty(),
                "{label}: nothing had completed when the room was cut off"
            );
        }
        let after = &run.after;
        for (table, rows) in &before.tables {
            let lost: Vec<_> = rows.difference(&after.tables[table]).take(2).collect();
            assert!(
                lost.is_empty(),
                "{label}: {table} rows written before the crash changed or vanished: {lost:?}"
            );
        }
        assert!(
            before.completed.is_subset(&after.completed),
            "{label}: an attempt that completed before the crash was rewritten"
        );
        assert!(
            before.completed_work.is_subset(&after.completed_work),
            "{label}: work completed before the crash was reopened"
        );
        assert!(
            run.completed_per_work.values().all(|n| *n == 1),
            "{label}: a work item completed more than once: {:?}",
            run.completed_per_work
        );
        // No accepted call is replayed: at most the calls in flight at the crash are
        // asked for again on top of what the uninterrupted run needed.
        assert!(
            run.served <= control.served + 12,
            "{label}: {} provider calls against {} in the control run",
            run.served,
            control.served
        );
    }
}

mod forge {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("forge_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::scenario(6, Scenario::Happy).await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers {
                scenario: Scenario::Happy,
                report_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }),
            starts: vec![BRIEF.to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod forecast {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("forecast_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec!["@all $forecast Will the launch happen by 2027-12-31?".to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod decide {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("decide_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec!["@all $decide Select service\n--\nBuild\nBuy".to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod negotiate {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("negotiate_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::scenario(6, Scenario::Happy).await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers {
                scenario: Scenario::Happy,
                report_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }),
            starts: vec![BRIEF.to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod blow_it_up {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("blow_it_up_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new(Scenario::Happy).await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers { scenario: Scenario::Happy, counts: std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())) }),
            starts: vec!["@all $blow_it_up ```All requests are accepted without authentication.``` -- access abuse".to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod readback {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("readback_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec![format!("@all $readback {SPEC} ")],
            human: vec![Some("1a"), None],
            _keep: Box::new(f),
        }
    }
}

mod debugging {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("debugging_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec!["@all $debug Network requests fail intermittently.".to_string()],
            human: vec![Some("O1 resolver returned no address")],
            _keep: Box::new(f),
        }
    }
}

mod trace {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("trace_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec![format!("@all $trace Retention \n--\n{SOURCE}")],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod rank {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("rank_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec![START.to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod tune {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("tune_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::scenario(6, Scenario::Happy).await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers { scenario: Scenario::Happy, calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)), executions: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)) }),
            starts: vec!["$TUNE_GENERATIONS=1 @all $tune PROMPT answer clearly\n--\nprovide helpful concise answers".to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
}

mod bass_ackwards_prd {
    #![allow(dead_code, unused)]
    // The mode's own workflow file is included whole so its fixture and scripted provider
    // are reused as written. Its tests become plain, unregistered functions here because
    // they already run in their own binary.
    mod tokio {
        pub use ::tokio::*;
        pub use ::tracing::instrument as test;
    }
    include!("bass_ackwards_prd_workflows.rs");
    pub async fn rig() -> crate::Rig {
        let f = std::rc::Rc::new(Fixture::new().await);
        crate::Rig {
            config: f.config.clone(),
            base_url: f.fake.base_url(),
            mount: {
                let f = f.clone();
                Box::new(move |m| {
                    let f = f.clone();
                    Box::pin(async move { m.mount(&f.fake.server).await })
                })
            },
            inner: std::sync::Arc::new(Answers),
            starts: vec!["@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts".to_string()],
            human: vec![],
            _keep: Box::new(f),
        }
    }
    /// The same mode with the optional final human review switched on.
    pub async fn review_rig() -> crate::Rig {
        let mut rig = rig().await;
        rig.starts.insert(0, "/set PRD_HUMAN_REVIEW 1".to_string());
        rig.human = vec![
            Some("Include the operations team's deployment constraints"),
            None,
        ];
        rig
    }
}

fn mode_rig<F: Fn() -> Local<Rig>>(f: F) -> impl Fn() -> Local<Rig> {
    f
}

macro_rules! recovers {
    ($test:ident, $mode:literal, $rig:path, $human:expr, $max:expr) => {
        #[tokio::test]
        async fn $test() {
            sweep($mode, mode_rig(|| Box::pin($rig())), $human, $max).await;
        }
    };
}
recovers!(forge_recovers_at_every_boundary, "forge", forge::rig, 0, 5);
recovers!(
    forecast_recovers_at_every_boundary,
    "forecast",
    forecast::rig,
    0,
    5
);
recovers!(
    decide_recovers_at_every_boundary,
    "decide",
    decide::rig,
    0,
    5
);
recovers!(
    negotiate_recovers_at_every_boundary,
    "negotiate",
    negotiate::rig,
    0,
    5
);
recovers!(
    blow_it_up_recovers_at_every_boundary,
    "blow_it_up",
    blow_it_up::rig,
    0,
    5
);
recovers!(
    readback_recovers_at_every_boundary_and_human_step,
    "readback",
    readback::rig,
    2,
    5
);
recovers!(
    debug_recovers_at_every_boundary_and_human_step,
    "debug",
    debugging::rig,
    2,
    5
);
recovers!(trace_recovers_at_every_boundary, "trace", trace::rig, 0, 5);
recovers!(rank_recovers_at_every_boundary, "rank", rank::rig, 0, 5);
recovers!(
    tune_recovers_at_every_boundary_without_repeating_an_execution,
    "tune",
    tune::rig,
    0,
    5
);
recovers!(
    prd_recovers_at_every_boundary,
    "bass_ackwards_prd",
    bass_ackwards_prd::rig,
    0,
    2
);
recovers!(
    prd_recovers_at_every_boundary_and_human_review,
    "bass_ackwards_prd review",
    bass_ackwards_prd::review_rig,
    2,
    0
);
