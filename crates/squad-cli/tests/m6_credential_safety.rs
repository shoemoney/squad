use std::process::Command;
const KEY: &str = "M6-CLI-SENTINEL-PRIVATE-CREDENTIAL";

#[test]
fn doctor_with_fake_credential_is_offline_redacted_and_does_not_open_the_room() {
    let dir = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_squad"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path())
        .env("OPENROUTER_API_KEY", KEY)
        .current_dir(dir.path())
        .args(["--data-dir"])
        .arg(dir.path().join("data"))
        .arg("doctor")
        .output()
        .unwrap();
    for bytes in [&result.stdout, &result.stderr] {
        assert!(!String::from_utf8_lossy(bytes).contains(KEY));
    }
    assert!(!dir.path().join("data/squad.db").exists());
    assert!(!dir.path().join("data/main.lock").exists());
}
