use serde_json::json;
use squad_protocol::{tune::*, *};
fn proposal() -> serde_json::Value {
    json!({"cases":[{"localId":"a","kind":"typical","input":"sample","expect":["correct"],"mustNot":[]}],"criteria":[{"localId":"r","name":"Accuracy","description":"Be correct"}]})
}
#[test]
fn generated_objects_are_strict_and_bounded() {
    let p = proposal();
    assert!(validate_artifact("tune.cases", &p).is_ok());
    let mut p = p;
    p["winner"] = json!("me");
    assert!(validate_artifact("tune.cases", &p).is_err());
    let mut p = proposal();
    p["cases"][0]["input"] = json!("word ".repeat(151));
    assert!(validate_artifact("tune.cases", &p).is_err());
    let mut p = proposal();
    let duplicate = p["cases"][0].clone();
    p["cases"].as_array_mut().unwrap().push(duplicate);
    assert!(validate_artifact("tune.cases", &p).is_err());
}
#[test]
fn freeze_cannot_invent_sources_or_take_three_from_one_member() {
    let p: TuneCaseProposal = serde_json::from_value(proposal()).unwrap();
    let proposals = (1..=4)
        .map(|i| TuneLabelledProposal {
            label: format!("P{i}"),
            proposal: p.clone(),
        })
        .collect();
    let input = TuneCaseMaterial::Freeze {
        prompt: "prompt".into(),
        task: "task".into(),
        proposals,
    };
    let f = json!({"cases":["P1:a","P2:a","P3:a","P4:a"],"dropped":[],"rubric":[{"name":"Accuracy","description":"Be correct","sourceRefs":["P1:r"]},{"name":"Clarity","description":"Be clear","sourceRefs":["P2:r"]}]});
    assert!(input.validate_output(&f).is_ok());
    let mut bad = f.clone();
    bad["cases"][0] = json!("P9:invented");
    assert!(input.validate_output(&bad).is_err());
    let mut bad = f;
    bad["rubric"][0]["sourceRefs"] = json!([]);
    assert!(input.validate_output(&bad).is_err());
}
#[test]
fn mutations_preserve_placeholder_count_and_are_distinct_from_parent() {
    let input = TuneCaseMaterial::Mutate {
        task: "task".into(),
        prompt: "Original {{input}}".into(),
        digest: TuneDigest { cases: vec![] },
        length_budget: 3,
        placeholders: 1,
    };
    for prompt in [
        "Original   {{input}}",
        "Different prompt",
        "Many more words than allowed {{input}}",
    ] {
        assert!(
            input
                .validate_output(
                    &json!({"prompt":prompt,"changes":[{"what":"change","why":"fix","targets":[]}]})
                )
                .is_err()
        );
    }
    assert!(input.validate_output(&json!({"prompt":"Improved {{input}}","changes":[{"what":"change","why":"fix","targets":[]}]})).is_ok());
    assert_eq!(length_budget("hello"), 151);
    assert_eq!(length_budget(&"w ".repeat(1200)), 1800);
}
#[test]
fn enabled_parser_example_and_named_executor_counts() {
    let ctx = ParseContext {
        roster: ["alice", "bob", "carol", "dave"]
            .map(|n| n.parse().unwrap())
            .to_vec(),
        ..Default::default()
    };
    if !ENABLED {
        assert_eq!(
            parse(
                "@all $tune Summarize clearly\n--\ntask",
                &SubmissionMeta::default(),
                &ctx
            )
            .unwrap_err()
            .code,
            ErrorCode::COMMAND_NOT_IMPLEMENTED
        );
        return;
    }
    assert!(matches!(
        parse(
            "@all $tune Summarize clearly\n--\nsummarize the input",
            &SubmissionMeta::default(),
            &ctx
        )
        .unwrap(),
        ParsedInput::Tune { .. }
    ));
    let p = parse(
        "@all $tune Summarize clearly\n--\ntask\n--\n@alice",
        &SubmissionMeta::default(),
        &ctx,
    )
    .unwrap();
    if let ParsedInput::Tune {
        participants,
        executor,
        ..
    } = p
    {
        assert_eq!(participants.len(), 3);
        assert_eq!(executor.unwrap().as_str(), "alice");
    } else {
        panic!("wrong AST");
    }
    let mut small = ctx;
    small.roster.pop();
    assert!(
        parse(
            "@all $tune prompt\n--\ntask\n--\n@alice",
            &SubmissionMeta::default(),
            &small
        )
        .is_err()
    );
    assert_eq!(
        parser::help_examples()
            .iter()
            .filter(|s| s.contains("$tune"))
            .count(),
        1
    );
}
#[test]
fn proposal_and_freeze_enforce_distinct_member_case_caps() {
    let mut p = proposal();
    let c = p["cases"][0].clone();
    p["cases"] = json!([c.clone(), c.clone(), c.clone(), c]);
    assert!(validate_artifact("tune.cases", &p).is_err());
    let mut p: TuneCaseProposal = serde_json::from_value(proposal()).unwrap();
    let mut b = p.cases[0].clone();
    b.local_id = "b".into();
    p.cases.push(b);
    let mut c = p.cases[0].clone();
    c.local_id = "c".into();
    p.cases.push(c);
    let q: TuneCaseProposal = serde_json::from_value(proposal()).unwrap();
    let input = TuneCaseMaterial::Freeze {
        prompt: "prompt".into(),
        task: "task".into(),
        proposals: vec![
            TuneLabelledProposal {
                label: "P1".into(),
                proposal: p,
            },
            TuneLabelledProposal {
                label: "P2".into(),
                proposal: q,
            },
        ],
    };
    let frozen = json!({"cases":["P1:a","P1:b","P1:c","P2:a"],"dropped":[],"rubric":[{"name":"Accuracy","description":"Be correct","sourceRefs":["P1:r"]},{"name":"Clarity","description":"Be clear","sourceRefs":["P2:r"]}]});
    assert!(input.validate_output(&frozen).is_err());
}
