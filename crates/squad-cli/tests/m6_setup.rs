#[test]
fn shipped_example_is_exactly_foundations_and_setup_preserves_private_credentials() {
    let spec = include_str!("../../../docs/prd/05-foundations-room-api-and-cli.md");
    let start = spec
        .split_once("The shipped `.env.example` is exactly:")
        .unwrap()
        .1;
    let example = start
        .split_once("```dotenv\n")
        .unwrap()
        .1
        .split_once("```\n")
        .unwrap()
        .0;
    assert_eq!(include_str!("../../../.env.example"), example);
    let output = std::process::Command::new("bash")
        .arg("scripts/test-setup.sh")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("setup.sh checks passed."));
}
