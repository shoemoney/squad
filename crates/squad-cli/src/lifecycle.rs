//! Coordinator binding points for room creation and the eventual TUI entry point.
use crate::{
    args::Send,
    output::Output,
    room_api::{OpenPolicy, RoomApi, RoomError},
};
use squad_protocol::config::SquadConfig;
use std::future::Future;

pub trait Runtime {
    type Room: RoomApi;
    fn interactive(&self, config: SquadConfig) -> impl Future<Output = Result<u8, RoomError>> {
        interactive_default(self, config)
    }
    fn open(
        &self,
        config: SquadConfig,
        policy: OpenPolicy,
    ) -> impl Future<Output = Result<Self::Room, RoomError>>;
    /// INTEGRATION STUB: implement with squad_tui::run(room).await once M4 lands.
    /// TUI restores the terminal; this layer owns room shutdown.
    fn tui(&self, room: &Self::Room) -> impl Future<Output = Result<(), RoomError>>;
}

pub async fn send(
    runtime: &impl Runtime,
    config: SquadConfig,
    args: &Send,
    output: &mut Output<'_>,
    confirm: impl FnMut() -> Option<bool>,
) -> Result<u8, RoomError> {
    let room = runtime.open(config, OpenPolicy::send(args.no_wait)).await?;
    crate::send::run(&room, args, output, confirm).await
}

pub async fn interactive(runtime: &impl Runtime, config: SquadConfig) -> Result<u8, RoomError> {
    runtime.interactive(config).await
}

async fn interactive_default(
    runtime: &(impl Runtime + ?Sized),
    config: SquadConfig,
) -> Result<u8, RoomError> {
    let room = runtime.open(config, OpenPolicy::interactive()).await?;
    let result = tokio::select! {
        result = runtime.tui(&room) => result.map(|()| 0),
        signal = tokio::signal::ctrl_c() => signal.map(|()| 130).map_err(|_| RoomError {
            code: squad_protocol::ErrorCode::NOT_READY,
            message: "Could not install interrupt handler.".into(), retryable: false,
        }),
    };
    let shutdown = room.shutdown().await;
    match result {
        Ok(code) => {
            shutdown?;
            Ok(code)
        }
        Err(error) => {
            let _ = shutdown;
            Err(error)
        }
    }
}
