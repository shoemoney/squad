//! Deterministic forecast aggregation and persisted script driving.
use squad_protocol::{forecast::*, *};
use std::collections::BTreeMap;
/// Structural numeric fields never enter the bundle. Scrub numeric prose too.
pub fn withhold(text: &str) -> String {
 let pattern=r"(?ix)(?:\b\d+(?:\.\d+)?\s*(?:%|percent\b))|(?:(?:\b0\.\d+|\B\.\d+|\b1\.0+)\b)|(?:\b\d+(?:\.\d+)?\s*(?:in|to|:)\s*\d+(?:\.\d+)?\b)";
 regex::Regex::new(pattern).expect("constant pattern").replace_all(text,"[number withheld]").into_owned()
}
pub fn blind(text:&str, setup:&ForecastSetup)->String {
 let mut tokens=setup.redaction_tokens.clone();
 tokens.extend(setup.participants.iter().map(ToString::to_string));
 tokens.extend(setup.models.values().map(|m|m.model.to_string()));
 let extra=tokens.iter().flat_map(|s|s.split(['/', '-', '_', '.', '~'])).filter(|s|s.chars().any(char::is_alphabetic)).map(str::to_owned).collect::<Vec<_>>();
 tokens.extend(extra); tokens.retain(|t|!t.is_empty()); tokens.sort_by_key(|t|std::cmp::Reverse(t.len()));tokens.dedup();
 if tokens.is_empty(){return text.into()}
 let pat=format!(r"(?i)\b(?:{})\b",tokens.iter().map(|s|regex::escape(s.trim_start_matches('~'))).collect::<Vec<_>>().join("|"));
 regex::Regex::new(&pat).expect("escaped tokens").replace_all(text,"[redacted]").into_owned()
}
pub fn bundle(recipient:&MemberNick, first:&BTreeMap<MemberNick,ForecastEstimate>, labels:&BTreeMap<MemberNick,String>,setup:&ForecastSetup)->ForecastBundle {
 let clean=|s:&str|withhold(&blind(s,setup));
 let mut entries=first.iter().filter(|(n,_)|*n!=recipient).map(|(n,e)|ForecastBundleEntry{
 label:labels[n].clone(), reference_class:clean(e.base_rate.reference_class.as_str()),
 factors:e.factors.iter().map(|f|ForecastFactor{direction:f.direction,text:Text::new(clean(f.text.as_str())).expect("nonempty")}).collect(), rationale:clean(e.rationale.as_str())}).collect::<Vec<_>>();
 entries.sort_by(|a,b|a.label.cmp(&b.label)); ForecastBundle{recipient:recipient.clone(),entries}
}
pub fn log_odds(p:u8)->f64 {let p=f64::from(p)/100.0;(p/(1.0-p)).ln()}
fn sigmoid(x:f64)->f64{1.0/(1.0+(-x).exp())}
/// Reject invalid quorum and endpoints rather than inventing probabilities.
pub fn aggregate(probabilities:&[u8])->Result<ForecastAggregate,&'static str> {
 let n=probabilities.len();
 if !(3..=12).contains(&n)||probabilities.iter().any(|p|!(1..=99).contains(p)){return Err("invalid forecast quorum or probability")}
 let mut logs=probabilities.iter().map(|p|log_odds(*p)).collect::<Vec<_>>();logs.sort_by(f64::total_cmp);
 let k=n/5;let trimmed=logs[k..n-k].to_vec();let l=trimmed.iter().sum::<f64>()/trimmed.len() as f64;
 let mean=logs.iter().sum::<f64>()/n as f64;
 let s=(logs.iter().map(|x|(x-mean).powi(2)).sum::<f64>()/(n-1) as f64).sqrt();
 let min_side=n.div_ceil(3);
 let largest=logs.windows(2).enumerate().max_by(|a,b|(a.1[1]-a.1[0]).total_cmp(&(b.1[1]-b.1[0])));
 let split=largest.and_then(|(i,w)|{let cut=i+1;(w[1]-w[0]>=2.0 && cut>=min_side && n-cut>=min_side).then(||ForecastSplit{lower:sigmoid(logs[..cut].iter().sum::<f64>()/cut as f64),higher:sigmoid(logs[cut..].iter().sum::<f64>()/(n-cut) as f64),cut})});
 Ok(ForecastAggregate{probabilities_pct:probabilities.to_vec(),sorted_log_odds:logs,k,trimmed_log_odds:trimmed,l,p:sigmoid(l),s,low:sigmoid(l-s),high:sigmoid(l+s),minimum_pct:*probabilities.iter().min().expect("quorum"),maximum_pct:*probabilities.iter().max().expect("quorum"),split})
}
pub fn uncited_move(first:u8,final_pct:u8,l1:f64,cited:&[String])->bool {
 cited.is_empty()&&(log_odds(final_pct)-l1).abs()<(log_odds(first)-l1).abs()
}
pub fn prepare(parsed:&ParsedInput)->Result<(),crate::prompts::PromptError>{
 if matches!(parsed,ParsedInput::Forecast{..}){Ok(())}else{Err(crate::prompts::PromptError::new("forecast",None,"not forecast input"))}
}
pub fn drive(router:&mut crate::router::Router)->squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
 let ids=router.store.call(|db|{let mut q=db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='forecast'")?;Ok(q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)})?;
 let mut outcomes=vec![];
 for raw in ids {
 let contest:ContestId=raw.parse().map_err(|_|squad_store::forecast::invalid())?;
 if router.engine_scope.as_ref().is_some_and(|s|!s.contains(&contest)){continue}
 for _ in 0..32 {
 let state=router.store.call(move |db|crate::router::forecast::load_state(db,contest))?;
 if state.state.is_terminal(){break}
 let id=EventId::new();let digest=squad_store::commit::digest(&state)?;
 let outcome=router.store.commit(id,"contest-engine".into(),digest,move|tx,g|crate::router::forecast::drive_move(tx,g,id,state))?;
 if outcome.events.is_empty(){break}outcomes.push(outcome);
 }
 }Ok(outcomes)
}
