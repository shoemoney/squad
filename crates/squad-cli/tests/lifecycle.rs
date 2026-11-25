mod support;
use squad_cli::{
    lifecycle::{self, Runtime},
    output::Output,
    room_api::{OpenPolicy, RoomError},
};
use squad_protocol::{ErrorCode, config::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use support::*;

#[derive(Default)]
struct FakeRuntime {
    policies: RefCell<Vec<OpenPolicy>>,
    shutdowns: Rc<Cell<u32>>,
    locked: bool,
}
impl Runtime for FakeRuntime {
    type Room = FakeRoom;
    async fn open(&self, _: SquadConfig, policy: OpenPolicy) -> Result<FakeRoom, RoomError> {
        self.policies.borrow_mut().push(policy);
        if self.locked {
            return Err(RoomError {
                code: ErrorCode::NOT_READY,
                message: "Room main is already open in another squad process (pid 42).".into(),
                retryable: false,
            });
        }
        Ok(FakeRoom {
            shutdowns: self.shutdowns.clone(),
            ..Default::default()
        })
    }
    async fn tui(&self, _: &FakeRoom) -> Result<(), RoomError> {
        Ok(())
    }
}
fn config(temp: &tempfile::TempDir) -> SquadConfig {
    let env = std::collections::BTreeMap::from([(
        "OPENROUTER_API_KEY".into(),
        "fixture-not-real".into(),
    )]);
    let mut options = LoadOptions::from_environment(temp.path().to_owned(), &env);
    options.data_dir = Some(temp.path().to_owned());
    EffectiveEnvironment::load(&options, &env)
        .validate(&options)
        .unwrap()
}

#[tokio::test]
async fn interactive_and_one_shot_open_with_distinct_dispatch_policies() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = FakeRuntime::default();
    assert_eq!(
        lifecycle::interactive(&runtime, config(&temp))
            .await
            .unwrap(),
        0
    );
    for no_wait in [false, true] {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        assert_eq!(
            lifecycle::send(
                &runtime,
                config(&temp),
                &args("@all hello", true, no_wait),
                &mut Output {
                    json: true,
                    stdout: &mut stdout,
                    stderr: &mut stderr
                },
                || None
            )
            .await
            .unwrap(),
            0
        );
    }
    assert_eq!(
        *runtime.policies.borrow(),
        [
            OpenPolicy::interactive(),
            OpenPolicy::send(false),
            OpenPolicy::send(true)
        ]
    );
    assert_eq!(runtime.shutdowns.get(), 3);
}

#[tokio::test]
async fn lock_failure_keeps_exact_diagnostic_and_does_not_attempt_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = FakeRuntime {
        locked: true,
        ..Default::default()
    };
    let error = lifecycle::interactive(&runtime, config(&temp))
        .await
        .unwrap_err();
    assert_eq!(
        error.message,
        "Room main is already open in another squad process (pid 42)."
    );
    assert_eq!(runtime.shutdowns.get(), 0);
}
