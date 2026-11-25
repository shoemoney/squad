use std::process::{Command, Stdio};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_commands_never_fetch_catalog_or_probe_aliases() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["doctor"],
        vec!["models"],
        vec!["--help"],
        vec!["send", "--help"],
        vec!["models", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_squad"))
            .env_clear()
            .env("HOME", root.path())
            .current_dir(root.path())
            .env("OPENROUTER_API_KEY", "fixture")
            .env("OPENROUTER_BASE_URL", format!("{}/api/v1/", server.uri()))
            .env("SQUAD_MODEL_A", "~fixture/member-latest")
            .env("SQUAD_JUDGE_MODEL", "~fixture/judge-latest")
            .env("SQUAD_ALIAS_CACHE_TTL_SECONDS", "0")
            .stdin(Stdio::null())
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(
        !root
            .path()
            .join(".local/share/squad/main/squad.db")
            .exists()
    );
}
