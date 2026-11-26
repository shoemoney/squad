use squad_core::export::tune::unified_diff;
#[test]
fn unified_diff_preserves_lines_and_absent_final_newlines() {
    assert_eq!(unified_diff("same", "same"), "");
    assert_eq!(
        unified_diff("old\n", "new\n"),
        "--- original\n+++ winner\n@@ -1,1 +1,1 @@\n-old\n+new\n"
    );
    let diff = unified_diff("old", "new");
    assert_eq!(diff.matches("\\ No newline at end of file").count(), 2);
}
#[test]
fn report_uses_the_winning_prompt_byte_for_byte_and_lineage() {
    use squad_protocol::{tune::*, *};
    use std::collections::BTreeMap;
    let prompt = "  exact winner\n\ntrailing  ";
    let setup = TuneSetup {
        prompt: "original".into(),
        task: "task".into(),
        participants: vec![],
        executor: ExecutorBinding {
            model: Text::new("fixture/model").unwrap(),
            effort: ReasoningEffort::Low,
        },
        generations: 1,
        length_budget: 151,
        rotation: vec![],
        seed: "seed".into(),
    };
    let c = TuneCandidate {
        id: "c0".into(),
        parent: None,
        generation: 0,
        author: None,
        prompt: "original".into(),
        changes: vec![],
        incomplete: false,
    };
    let winner = TuneCandidate {
        id: "c1".into(),
        parent: Some("c0".into()),
        generation: 1,
        author: Some("alice".parse().unwrap()),
        prompt: prompt.into(),
        changes: vec![TuneChange {
            what: "Added precision".into(),
            why: "Pass more cases".into(),
            targets: vec!["T1".into()],
        }],
        incomplete: false,
    };
    let run = squad_store::tune::TuneRun {
        setup,
        proposals: BTreeMap::new(),
        suite: None,
        candidates: vec![c, winner],
        champion: "c1".into(),
        champions: vec!["c0".into(), "c1".into()],
        generation: 1,
        generations: vec![],
        sheets: vec![],
        digests: vec![],
        holdout: None,
        report: Some(TuneReport {
            reason: "tuned".into(),
            result_line: "Tuned: 0.50 to 0.75 on held-out cases.".into(),
            winner: "c1".into(),
            prompt: prompt.into(),
            diff: unified_diff("original", prompt),
        }),
        jobs: vec![],
        maps: BTreeMap::new(),
        finalists: vec![],
        skipped: vec![],
        skipped_members: vec![],
        redaction: vec![],
    };
    let report = squad_core::export::tune::report(&run);
    assert!(report.contains(prompt));
    assert!(report.contains("Added precision"));
    assert!(report.contains("alice"));
    assert!(report.contains("Generation 1"));
}
