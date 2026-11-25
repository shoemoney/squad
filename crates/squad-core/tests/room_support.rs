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
    Mock::given(method("GET")).and(path("/models")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[{"id":"test/model","context_length":10000,"pricing":{"prompt":"0.000001","completion":"0.000002"}}]}))).mount(&server).await;
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
