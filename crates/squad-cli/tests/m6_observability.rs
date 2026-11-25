#[path = "../src/logging.rs"]
mod logging;
#[allow(dead_code)]
#[path = "../src/paths.rs"]
mod paths;
use squad_protocol::{SquadConfig, config::LoadOptions};

#[test]
fn file_json_correlates_retry_and_case_spans_and_redacts_credentials_and_reasoning() {
    let dir = tempfile::tempdir().unwrap();
    let config = SquadConfig::load_with_env(
        &LoadOptions {
            env_file: None,
            data_dir: None,
            working_dir: dir.path().into(),
            config_dir: None,
            default_data_dir: dir.path().join("data"),
        },
        &std::collections::BTreeMap::from([(
            "OPENROUTER_API_KEY".into(),
            "M6-LOG-SENTINEL".into(),
        )]),
    )
    .unwrap();
    let key = config.openrouter_api_key;
    logging::init(
        dir.path(),
        logging::level(squad_protocol::config::LogLevel::Debug),
        &key,
    )
    .unwrap();
    for attempt in [1_u64, 2] {
        let span = tracing::info_span!(
            "judge",
            request_id = "r1",
            event_id = "e1",
            work_id = "w1",
            attempt,
            contest_id = "d1",
            case_id = "c1",
            api_key = "M6-LOG-SENTINEL"
        );
        let _guard = span.enter();
        tracing::info!(
            reasoning = "PRIVATE REASONING",
            body = "PRIVATE BODY",
            prompt = "PRIVATE PROMPT",
            model = "fixture/judge",
            input_tokens = 11_u64,
            output_tokens = 17_u64,
            "M6-LOG-SENTINEL completed"
        );
    }
    let bytes = std::fs::read_to_string(dir.path().join("logs/squad.jsonl")).unwrap();
    for forbidden in [
        "M6-LOG-SENTINEL",
        "PRIVATE REASONING",
        "PRIVATE BODY",
        "PRIVATE PROMPT",
    ] {
        assert!(!bytes.contains(forbidden));
    }
    let rows: Vec<serde_json::Value> = bytes
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    for (index, row) in rows.iter().enumerate() {
        assert!(row["timestamp_ms"].is_number());
        assert_eq!(row["level"], "INFO");
        let span = &row["spans"][0]["fields"];
        for (name, value) in [
            ("request_id", "r1"),
            ("event_id", "e1"),
            ("work_id", "w1"),
            ("contest_id", "d1"),
            ("case_id", "c1"),
        ] {
            assert_eq!(span[name], value);
        }
        assert_eq!(span["attempt"], index + 1);
        assert_eq!(row["fields"]["input_tokens"], 11);
        assert_eq!(row["fields"]["output_tokens"], 17);
    }
}
