mod support;
use squad_cli::output::Output;

#[test]
fn human_final_and_errors_strip_terminal_controls() {
    let attack = "before\x1b]52;c;Y2xpcGJvYXJk\x07\x1b[2J\u{9b}31m\u{9d}hidden\u{9c}after";
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut output = Output {
        json: false,
        stdout: &mut stdout,
        stderr: &mut stderr,
    };
    output.event(&support::reply(1, attack)).unwrap();
    output
        .error(None, squad_protocol::ErrorCode::NOT_READY, attack, false)
        .unwrap();
    assert_eq!(String::from_utf8(stdout).unwrap(), "alpha: beforeafter\n");
    assert_eq!(String::from_utf8(stderr).unwrap(), "beforeafter\n");
}

#[test]
fn json_final_remains_roundtrippable_without_raw_escape_bytes() {
    let event = support::reply(1, "before\x1b]52;c;data\x07\u{9d}52;c;data\u{9c}after");
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    Output {
        json: true,
        stdout: &mut stdout,
        stderr: &mut stderr,
    }
    .event(&event)
    .unwrap();
    assert!(!stdout.contains(&0x1b));
    assert!(
        !String::from_utf8_lossy(&stdout)
            .chars()
            .any(|c| ('\u{80}'..='\u{9f}').contains(&c))
    );
    let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(value["event"], serde_json::to_value(event).unwrap());
}

#[test]
fn export_display_strips_osc_but_saved_artifact_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let attack = "before\x1b]52;c;YXR0YWNr\x07after";
    let _db = support::database(dir.path(), vec![support::reply(1, attack)]);
    let mut bytes = Vec::new();
    let mut args = squad_cli::args::Export {
        scope: squad_protocol::ExportScope::Room,
        id: None,
        output: None,
    };
    squad_cli::export::run(dir.path(), &args, &mut bytes).unwrap();
    assert!(!bytes.contains(&0x1b));
    assert!(String::from_utf8(bytes).unwrap().contains("beforeafter"));
    let artifact = dir.path().join("transcript.md");
    args.output = Some(artifact.clone());
    squad_cli::export::run(dir.path(), &args, &mut Vec::new()).unwrap();
    assert!(std::fs::read_to_string(artifact).unwrap().contains(attack));
}
