mod support;
use serde_json::json;
use squad_cli::output::Output;
use support::*;

#[tokio::test]
async fn mode_artifacts_reports_and_export_ready_scopes_keep_shared_schema() {
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    let mut records = Vec::new();
    for mode in ["census", "crosscheck", "survey", "probe"] {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut output = Output {
            json: true,
            stdout: &mut stdout,
            stderr: &mut stderr,
        };
        for fixture in [
            event(
                2,
                "contest.artifact",
                json!({"contestId":ID,"stepId":ID,"artifactKind":mode,"sourceRefs":[ID],"workId":ID,"data":{"partial":true,"skippedMembers":["beta"]}}),
            ),
            event(
                3,
                "contest.report",
                json!({"contestId":ID,"stepId":ID,"artifactEventIds":[ID],"skippedWorkIds":[ID],"data":{"mode":mode,"partial":true}}),
            ),
        ] {
            output.event(&fixture).unwrap();
        }
        records.extend(
            String::from_utf8(stdout)
                .unwrap()
                .lines()
                .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap()),
        );
        let (code, text, _) = send(
            &FakeRoom::default(),
            &args(&format!("/export {mode} {ID}"), true, false),
            None,
        )
        .await;
        assert_eq!(code, 0);
        let mut ready: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(ready["scope"], mode);
        assert_eq!(ready["id"], ID);
        ready["requestId"] = json!("<request>");
        ready["path"] = json!("<export path>");
        records.push(ready);
    }
    std::env::set_current_dir(previous).unwrap();
    insta::assert_json_snapshot!(records);
}
