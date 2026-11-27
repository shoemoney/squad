use serde_json::json;
use squad_agents::agents::tune::enabled;
use squad_protocol::{tune::*, *};
#[test]
fn tune_handler_is_registered_and_schema_has_no_authoritative_winner() {
    assert_eq!(enabled(), squad_protocol::tune::ENABLED);
    for kind in [
        WorkKind::TuneCases,
        WorkKind::TuneFreeze,
        WorkKind::TuneScore,
        WorkKind::TuneMutate,
    ] {
        let schema = output_schema(&kind).unwrap();
        let object = schema["properties"].as_object().unwrap();
        assert!(!object.contains_key("winner"));
        assert!(!object.contains_key("total"));
    }
    assert!(output_schema(&WorkKind::TuneExecute).is_none());
}
#[test]
fn score_requests_require_every_case_label_and_criterion() {
    let case = TuneScoringCase {
        case_label: "T1".into(),
        case: TuneTestCase {
            local_id: "a".into(),
            kind: TuneCaseKind::Typical,
            input: "secret input".into(),
            expect: vec!["works".into()],
            must_not: vec![],
        },
        outputs: vec![TuneScoringOutput {
            label: "A".into(),
            output: "answer".into(),
        }],
    };
    let rubric = ["Accuracy", "Clarity"]
        .map(|n| TuneRubricCriterion {
            name: n.into(),
            description: "correct and clear".into(),
            source_refs: vec!["P1:r".into()],
        })
        .to_vec();
    let input = TuneCaseMaterial::Score {
        task: "task".into(),
        rubric,
        cases: vec![case],
    };
    let good = json!({"cases":[{"caseLabel":"T1","outputs":[{"label":"A","scores":[{"criterion":"Accuracy","score":2},{"criterion":"Clarity","score":4}],"note":"Partly useful answer"}]}]});
    input.validate_output(&good).unwrap();
    for (path, value) in [("label", json!("B")), ("note", json!(""))] {
        let mut bad = good.clone();
        bad["cases"][0]["outputs"][0][path] = value;
        assert!(input.validate_output(&bad).is_err());
    }
    let mut bad = good;
    bad["cases"][0]["outputs"][0]["scores"][1]["score"] = json!(5);
    assert!(input.validate_output(&bad).is_err());
}
