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
    let event = support::reply(1, "before\x1b]52;c;data\x07after");
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    Output {
        json: true,
        stdout: &mut stdout,
        stderr: &mut stderr,
    }
    .event(&event)
    .unwrap();
    assert!(!stdout.contains(&0x1b));
    let value: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(value["event"], serde_json::to_value(event).unwrap());
}
