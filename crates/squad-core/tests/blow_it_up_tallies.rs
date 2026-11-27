use serde_json::{Value, json};
use squad_core::{
    orchestrator::blow_it_up::{rank, tally},
    router::blow_it_up::validate_output,
};
use squad_protocol::{WorkKind, blow_it_up::*};
fn pooled(id: &str) -> PooledVector {
    serde_json::from_value(json!({"vectorId":id,"title":"Access","targetQuote":"No authentication is required.","exploit":"Anyone writes","severity":"high","likelihood":"likely","blastRadius":"All data","addition":false})).unwrap()
}
fn ballots(votes: &[&str]) -> Vec<(String, BlowItUpChallenge)> {
    votes.iter().enumerate().map(|(i,v)|(format!("member{i}"),serde_json::from_value(json!({"verdicts":[{"vectorId":"a","verdict":v,"reason":"Target evidence","duplicateOf":null,"defeatingQuote":if *v=="false_positive"{json!("No authentication is required.")}else{Value::Null}}],"additions":[]})).unwrap())).collect()
}
#[test]
fn integer_majorities_include_exact_half_and_missing_ballots_are_not_votes() {
    let pool = BlowItUpPool {
        vectors: vec![pooled("a")],
        provenance: vec![],
    };
    for (votes, status) in [
        (vec![], VectorStatus::Unchallenged),
        (vec!["false_positive"], VectorStatus::Unchallenged),
        (vec!["false_positive", "holds"], VectorStatus::Standing),
        (vec!["weak", "holds"], VectorStatus::Standing),
        (
            vec!["false_positive", "false_positive", "holds"],
            VectorStatus::Killed,
        ),
        (
            vec!["false_positive", "weak", "holds"],
            VectorStatus::Contested,
        ),
        (vec!["weak", "weak"], VectorStatus::Contested),
        (vec!["holds", "holds"], VectorStatus::Standing),
    ] {
        let result = tally(&pool, &ballots(&votes));
        assert_eq!(result.vectors[0].status, status, "{votes:?}");
        assert_eq!(result.vectors[0].accepted, votes.len() as u32);
    }
}
fn finding(id: &str, sources: &[&str], sev: &str, lik: &str) -> Value {
    json!({"findingId":id,"title":id,"sourceVectorIds":sources,"severity":sev,"likelihood":lik,"exploit":"Anyone writes","blastRadius":"All data","mitigation":"Authenticate","rebuttalStatus":"survived","residual":"Credential abuse","challengeRejection":null})
}
#[test]
fn ranking_uses_product_then_distinct_blind_attackers_then_judge_order() {
    let mut pool = BlowItUpPool {
        vectors: vec![pooled("a"), pooled("b"), pooled("c"), pooled("addition")],
        provenance: vec![],
    };
    for (id, n, addition) in [
        ("a", "alpha", false),
        ("b", "beta", false),
        ("c", "alpha", false),
        ("addition", "gamma", true),
    ] {
        pool.provenance.push(VectorProvenance {
            vector_id: id.parse().unwrap(),
            attacker: n.parse().unwrap(),
            local_id: "local".parse().unwrap(),
            addition,
        });
    }
    pool.vectors[3].addition = true;
    let r:BlowItUpRuling=serde_json::from_value(json!({"findings":[finding("one",&["a"],"high","possible"),finding("two",&["a","b"],"medium","likely"),finding("three",&["a","c"],"high","possible"),finding("top",&["addition"],"critical","likely")],"killedNotes":[]})).unwrap();
    let rows = rank(&r, &pool);
    assert_eq!(
        rows.iter()
            .map(|r| r.finding.finding_id.as_str())
            .collect::<Vec<_>>(),
        ["top", "two", "one", "three"]
    );
    assert_eq!(rows[0].finder_count, 0);
    assert!(rows[0].addition);
    assert_eq!(rows[1].finder_count, 2);
    assert_eq!(rows[3].finder_count, 1);
    assert!(rows[3].lone);
}
#[test]
fn quote_and_reference_validation_uses_the_shared_matcher() {
    let data = json!({"target":"No authentication is required. The system verifies every write.","vectors":[pooled("a")],"wordLimit":1200});
    let mut output = json!({"verdicts":[{"vectorId":"a","verdict":"false_positive","reason":"Writes are verified","duplicateOf":null,"defeatingQuote":"The system verifies every write."}],"additions":[]});
    assert!(validate_output(&WorkKind::BlowItUpChallenge, &data, &output).is_ok());
    output["verdicts"][0]["defeatingQuote"] = json!("Invented target text");
    assert!(validate_output(&WorkKind::BlowItUpChallenge, &data, &output).is_err());
    output["verdicts"][0]["defeatingQuote"] = json!("No ... required.");
    assert!(validate_output(&WorkKind::BlowItUpChallenge, &data, &output).is_err());
    output["verdicts"][0]["vectorId"] = json!("own-hidden");
    assert!(validate_output(&WorkKind::BlowItUpChallenge, &data, &output).is_err());
}
