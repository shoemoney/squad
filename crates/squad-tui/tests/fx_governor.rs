use squad_tui::fx::*;
use std::time::Duration;
fn sample(ms: u64, bytes: u64, wait: u64) -> FrameSample {
    FrameSample {
        draw_and_flush: Duration::from_millis(ms),
        bytes_per_second: bytes,
        input_wait: Duration::from_millis(wait),
    }
}
#[test]
fn threshold_frame_counts_are_consecutive_and_strict() {
    for (ms, frames, grade) in [(9, 15, Grade::G1), (15, 15, Grade::G2), (23, 10, Grade::G3)] {
        let mut governor = Governor::default();
        for frame in 1..frames {
            assert!(governor.observe(sample(ms, 0, 0), Duration::from_millis(frame * 16)) < grade);
        }
        assert_eq!(
            governor.observe(sample(ms, 0, 0), Duration::from_millis(frames * 16)),
            grade
        );
    }
    for ms in [8, 14, 22] {
        let mut governor = Governor::default();
        for _ in 0..30 {
            governor.observe(sample(ms, 0, 0), Duration::ZERO);
        }
        assert_eq!(
            governor.grade(),
            match ms {
                8 => Grade::G0,
                14 => Grade::G1,
                _ => Grade::G2,
            }
        );
    }
}
#[test]
fn ewma_uses_alpha_point_two() {
    let mut governor = Governor::default();
    governor.observe(sample(5, 0, 0), Duration::ZERO);
    governor.observe(sample(15, 0, 0), Duration::ZERO);
    assert!((governor.ewma_ms() - 7.).abs() < 1e-9);
}
#[test]
fn bytes_and_input_latency_are_immediate_pressure() {
    for (bytes, wait, grade) in [
        (120_000, 50, Grade::G0),
        (120_001, 0, Grade::G1),
        (250_000, 0, Grade::G1),
        (250_001, 0, Grade::G2),
        (0, 51, Grade::G3),
    ] {
        assert_eq!(
            Governor::default().observe(sample(1, bytes, wait), Duration::ZERO),
            grade
        );
    }
}
#[test]
fn recovery_waits_three_seconds_and_moves_only_one_grade() {
    let mut governor = Governor::default();
    governor.observe(sample(1, 0, 51), Duration::ZERO);
    governor.observe(sample(1, 0, 0), Duration::ZERO);
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_millis(2999)),
        Grade::G3
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(3)),
        Grade::G2
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(3)),
        Grade::G2
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(6)),
        Grade::G1
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(6)),
        Grade::G1
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(9)),
        Grade::G0
    );
}
#[test]
fn renewed_pressure_resets_hysteresis() {
    let mut governor = Governor::default();
    governor.observe(sample(1, 121_000, 0), Duration::ZERO);
    governor.observe(sample(1, 0, 0), Duration::ZERO);
    governor.observe(sample(1, 121_000, 0), Duration::from_secs(2));
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(3)),
        Grade::G1
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(5)),
        Grade::G1
    );
    assert_eq!(
        governor.observe(sample(1, 0, 0), Duration::from_secs(6)),
        Grade::G0
    );
}
#[test]
fn shedding_table_and_explicit_level_are_composed() {
    for (bytes, wait, grade, ambience, particles, passes, fps) in [
        (0, 0, Grade::G0, true, 256, true, 60),
        (121_000, 0, Grade::G1, false, 128, true, 30),
        (251_000, 0, Grade::G2, false, 0, false, 30),
        (0, 51, Grade::G3, false, 0, false, 15),
    ] {
        let mut governor = Governor::default();
        governor.observe(sample(1, bytes, wait), Duration::ZERO);
        assert_eq!(governor.grade(), grade);
        let policy = governor.policy(FxLevel::Full, true);
        assert_eq!(
            (
                policy.ambience,
                policy.particle_limit,
                policy.full_buffer,
                policy.fps
            ),
            (ambience, particles, passes, fps)
        );
        assert!(!governor.policy(FxLevel::Full, false).full_buffer);
        let lite = governor.policy(FxLevel::Lite, true);
        assert!(!lite.ambience && !lite.full_buffer && lite.fade_only);
        assert_eq!(lite.particle_limit, 0);
        assert!(lite.fps <= 30);
        let off = governor.policy(FxLevel::Off, true);
        assert!(off.snap && !off.animate && !off.full_buffer);
    }
}
