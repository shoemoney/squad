use squad_protocol::*;
use serde_json::json;
fn text(s: &str) -> Text { Text::new(s).unwrap() }
fn check() -> ProbeItem<ProbeCheck> { ProbeItem { id:CellId::new(), data:ProbeCheck{check:text("Inspect indicator"),procedure:text("Read without changing anything"),measurement:text("Indicator color"),outcomes:vec![ProbeOutcome{id:text("red"),description:text("Red")},ProbeOutcome{id:text("green"),description:text("Green")}],effort:text("one minute"),safety_notes:vec![text("Read only")]}} }
#[test]
fn prediction_requires_exact_coverage_and_explicit_inability() {
 let c=check(); let p=ProbePrediction{predictions:vec![ProbePredictionItem{check_id:c.id,predictable:false,expected_outcome_ids:vec![],expected_observation:None,reason:text("Unrelated indicator")}]};
 assert!(p.validate_against(std::slice::from_ref(&c)).is_ok());
 let mut bad=p.clone(); bad.predictions[0].expected_observation=Some(text("Red")); assert!(bad.validate_against(std::slice::from_ref(&c)).is_err());
 let mut bad=p.clone(); bad.predictions.push(p.predictions[0].clone()); assert!(bad.validate_against(&[c]).is_err());
 assert!(validation::decode::<ProbePrediction>(&json!({"predictions":[{"checkId":p.predictions[0].check_id,"predictable":false,"expectedOutcomeIds":[],"reason":"Unknown"}]})).is_err());
}
#[test]
fn split_and_call_counts_are_authoritative() {
 let members:Vec<MemberNick>=(0..6).map(|i|format!("m{i}").parse().unwrap()).collect();
 let mut setup=ProbeSetup{contest_id:ContestId::new(),brief:ProbeBrief{observation:text("Noise"),ruled_out:"Fan".into(),inspectable:text("Exterior")},participants:members.clone(),explainers:members[..3].to_vec(),checkers:members[3..].to_vec(),sitting_out:None};
 assert!(setup.validate().is_ok()); assert_eq!((setup.calls(),setup.attempts()),(10,21));
 setup.checkers[0]=setup.explainers[0].clone(); assert!(setup.validate().is_err());
}
#[test]
fn provider_outputs_reject_authority_and_attribution() {
 for extra in ["winner","author","eventId","riskAccepted","weightedTotals"] {
 let mut value=json!({"explanation":"Loose connection","keyAssumption":"Intermittent power","limitations":[]}); value[extra]=json!("forged"); assert!(validation::decode::<ProbeExplanation>(&value).is_err());
 }
 let mut c=check(); c.data.outcomes[1].id=c.data.outcomes[0].id.clone(); assert!(c.data.validate().is_err());
}
