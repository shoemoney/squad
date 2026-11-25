use ratatui::{Terminal, backend::TestBackend};
use squad_tui::{
    app::App,
    event_loop,
    fx::{FrameSample, FxLevel, Governor, Grade, TerminalCaps},
    session::FakeRoom,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

#[tokio::test(start_paused = true)]
async fn fx_off_and_no_color_idle_rooms_disarm_every_animation_timer() {
    for (name, value) in [("SQUAD_FX", "off"), ("NO_COLOR", "1")] {
        let caps = TerminalCaps::resolve(
            |key| (key == name).then(|| value.into()),
            false,
            (80, 24).into(),
        )
        .unwrap();
        let mut app = App::new(caps);
        app.composed.started = true;
        let (tx, mut rx) = mpsc::channel(1);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            drop(tx);
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        event_loop::run(&mut app, Arc::new(FakeRoom::new()), &mut terminal, &mut rx)
            .await
            .unwrap();
        assert_eq!(app.draws, 1, "idle frames for {name}");
    }
}

#[test]
fn full_effects_shed_to_fifteen_fps_under_sustained_pressure() {
    let mut governor = Governor::default();
    for frame in 0..10 {
        governor.observe(
            FrameSample {
                draw_and_flush: Duration::from_millis(23),
                bytes_per_second: 300000,
                input_wait: Duration::ZERO,
            },
            Duration::from_millis(frame * 25),
        );
    }
    assert_eq!(governor.grade(), Grade::G3);
    let policy = governor.policy(FxLevel::Full, true);
    assert_eq!(policy.fps, 15);
    assert!(!policy.ambience);
    assert!(!policy.full_buffer);
    assert_eq!(policy.particle_limit, 0);
}
