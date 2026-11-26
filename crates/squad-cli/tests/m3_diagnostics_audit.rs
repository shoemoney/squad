mod support;
use serde_json::{Value, json};
use squad_cli::buslog::BusLog;
use squad_core::bus::diagnostics;
use support::*;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn mode_and_pick_diagnostics_keep_public_identities_without_payloads() {
    for bodies in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bus.jsonl");
        let log = BusLog::open(&path, bodies, vec!["CREDENTIAL_CANARY".into()]).unwrap();
        let subscriber = tracing_subscriber::registry().with(log.layer());
        tracing::subscriber::with_default(subscriber, || {
            for mode in ["census", "crosscheck", "survey", "probe"] {
                diagnostics::commit(&event(
                    1,
                    "contest.step",
                    json!({
                        "contestId":ID,"stepId":ID,"ordinal":1,"scriptVersion":mode,
                        "moveName":"fanout","inputs":[ID],"assignments":[],
                    }),
                ));
                diagnostics::commit(&event(
                    2,
                    "contest.artifact",
                    json!({
                        "contestId":ID,"stepId":ID,"artifactKind":mode,"sourceRefs":[],
                        "data":{"systemPrompt":"PRIVATE_PROMPT","labelMap":"BLIND_MAP",
                        "context":"PRIVATE_CONTEXT","key":"CREDENTIAL_CANARY"},
                    }),
                ));
            }
            diagnostics::commit(&reply(3, "PRIVATE_ANSWER"));
            diagnostics::commit(&event(
                4,
                "contest.pick_requested",
                json!({
                    "contestId":ID,"kind":"grill","questionMessageId":ID,
                    "eligible":["alpha","beta"],"requiredPicks":1,"banner":"Choose",
                }),
            ));
            diagnostics::commit(&event(
                5,
                "case.opened",
                json!({
                    "caseId":ID,"judgeInvocationId":ID,"judgeModel":"fixture/judge",
                    "material":{"kind":"standalone","messages":[],"question":"PRIVATE_QUESTION"},
                }),
            ));
        });
        let text = std::fs::read_to_string(path).unwrap();
        for secret in ["PRIVATE_", "BLIND_MAP", "CREDENTIAL_CANARY", "labelMap"] {
            assert!(!text.contains(secret), "{secret}: {text}");
        }
        let rows: Vec<Value> = text
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 11);
        for row in &rows {
            assert_eq!(row["fields"]["event_id"].as_str().unwrap().len(), 36);
            assert_eq!(row["fields"]["correlation_id"], ID);
        }
        for row in &rows[..8] {
            assert_eq!(row["fields"]["contest_id"], ID);
            assert_eq!(row["fields"]["step_id"], ID);
        }
        assert_eq!(rows[8]["fields"]["work_id"], ID);
        assert_eq!(rows[8]["fields"]["attempt"], 1);
        assert_eq!(rows[9]["fields"]["contest_id"], ID);
        assert_eq!(rows[10]["fields"]["case_id"], ID);
    }
}
