use serde_json::json;
use squad_core::orchestrator::readback::*;
use squad_protocol::{readback::*, *};
use std::collections::{BTreeMap, BTreeSet};
const OLD: &str =
    "The service retains records. The operator deletes backups. The client retries requests.";
fn ledger() -> ReadbackLedger {
    serde_json::from_value(json!({"entries":[{"entryId":"one","passage":"The service retains records.","question":"For how long?","readings":[{"readingId":"a","text":"Thirty days","sourceLabels":["Reader 1"]},{"readingId":"b","text":"Forever","sourceLabels":["Reader 2"]}],"behaviourChange":"Records expire or persist"},{"entryId":"two","passage":"The operator deletes backups.","question":"When?","readings":[{"readingId":"a","text":"Daily","sourceLabels":["Reader 1"]},{"readingId":"b","text":"On request","sourceLabels":["Reader 2"]}],"behaviourChange":"Backup lifetime differs"}]})).unwrap()
}
#[test]
fn partial_rewrite_preserves_every_other_byte_and_exact_coverage() {
    let l = ledger();
    let choices = BTreeMap::from([("one".into(), "Thirty days".into())]);
    let good = ReadbackRewrite {
        spec: OLD.replacen(
            "The service retains records.",
            "The service retains records for thirty days.",
            1,
        ),
        changes: vec![ReadbackChange {
            entry_id: "one".into(),
            before: "The service retains records.".into(),
            after: "The service retains records for thirty days.".into(),
        }],
    };
    let check = |r: &ReadbackRewrite| verify_rewrite(r, OLD, &l, &choices, OLD.len(), 1200);
    check(&good).unwrap();
    let mut invalid = good.clone();
    invalid.spec = invalid.spec.replace(
        "The operator deletes backups.",
        "The operator always keeps backups.",
    );
    assert!(check(&invalid).is_err());
    let mut invalid = good.clone();
    invalid.spec.push(' ');
    assert!(check(&invalid).is_err());
    let mut invalid = good.clone();
    invalid.changes.clear();
    assert!(check(&invalid).is_err());
    let mut invalid = good.clone();
    invalid.changes[0].entry_id = "two".into();
    assert!(check(&invalid).is_err());
    let mut invalid = good.clone();
    invalid.changes[0].before = "Fabricated passage".into();
    assert!(check(&invalid).is_err());
    assert!(verify_rewrite(&good, OLD, &l, &choices, 5, 1200).is_err());
    assert!(verify_rewrite(&good, OLD, &l, &choices, OLD.len(), 2).is_err());
}
#[test]
fn verbatim_and_reference_validation_and_hand_counted_bounds() {
    let l = ledger();
    let labels = BTreeSet::from(["Reader 1".into(), "Reader 2".into()]);
    verify_ledger(&l, OLD, &labels).unwrap();
    let mut wrong = l.clone();
    wrong.entries[0].passage = "Invented passage that is absent".into();
    assert!(verify_ledger(&wrong, OLD, &labels).is_err());
    assert!(verify_ledger(&l, OLD, &BTreeSet::new()).is_err());
    assert_eq!(call_bounds(6, 2), (16, 36));
    let rounds = vec![squad_store::readback::Round {
        spec: OLD.into(),
        interpretations: BTreeMap::from([(
            "Reader 1".into(),
            validation::decode(
                &json!({"build":"Service","behaviours":[],"edgeCases":[],"assumptions":[]}),
            )
            .unwrap(),
        )]),
        ledger: Some(l),
        choices: Some(ReadbackChoices {
            answers: BTreeMap::from([("one".into(), "Thirty days".into())]),
            raw_answers: vec![],
        }),
        cut: 3,
        ..Default::default()
    }];
    let c = convergence(&rounds);
    assert_eq!(
        (
            c[0].accepted_interpretations,
            c[0].ledger_entries,
            c[0].resolved,
            c[0].open,
            c[0].cut
        ),
        (1, 2, 1, 1, 3)
    );
}
