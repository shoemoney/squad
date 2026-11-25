use squad_protocol::*;
use serde_json::json;
#[test]
fn strict_hunts_and_citations() {
    let good = json!({"gaps":[{"localId":"g1","omission":"backups","consequence":"loss"}]});
    let hunt: CensusHuntOutput = validation::decode(&good).unwrap();
    hunt.validate().unwrap();
    let mut duplicate = hunt.clone(); duplicate.gaps.push(hunt.gaps[0].clone());
    assert!(duplicate.validate().is_err());
    let mut extra = good; extra["finderCount"] = json!(4);
    assert!(validation::decode::<CensusHuntOutput>(&extra).is_err());
    let gap = CensusGap { id: Text::new("g1").unwrap(), omission: Text::new("backups").unwrap(), consequence: Text::new("loss").unwrap(), lenses: vec![Text::new("failure modes").unwrap()], source_find_ids: vec![Text::new("find1").unwrap()] };
    let rating = json!({"ratings":[{"gapId":"g1","rating":"covered","reason":"addressed","citation":{"location":"line 1","quote":"back up daily"}}],"additions":[]});
    let r: CensusRatingOutput = validation::decode(&rating).unwrap();
    assert!(r.validate_against(std::slice::from_ref(&gap), "back up daily").is_ok());
    assert!(r.validate_against(&[gap], "no backups").is_err());
    assert!(validation::decode::<CensusGapRating>(&json!({"gapId":"g1","rating":"real","reason":"missing"})).is_err());
}
#[test]
fn source_coverage_and_chao1() {
    let merge: CensusMergeOutput = validation::decode(&json!({"ledger":[{"id":"g1","omission":"backup","consequence":"loss","lenses":["failure modes"],"sourceFindIds":["f1","f2"]}]})).unwrap();
    assert!(merge.validate_against(&["f1".into(),"f2".into()]).is_ok());
    assert!(merge.validate_against(&["f1".into()]).is_err());
    assert_eq!(census_chao1(3,2), (2.25,"doubletons"));
    assert_eq!(census_chao1(3,0), (3.0,"zero_doubletons"));
    assert_eq!(census_chao1(0,0).0,0.0);
}
