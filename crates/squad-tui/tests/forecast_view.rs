use squad_protocol::*;
use squad_tui::widgets::forecast::*;
fn state()->ForecastState{ForecastState{contest_id:ContestId::new(),state_version:1,state:ForecastStatus::Forecasting,step_id:None,round:0,artifact_event_ids:vec![],terminal_reason:None}}
#[test]
fn banner_dividers_and_replay_versions(){
 let mut s=state();let mut v=View::default();v.apply(EventId::new(),&EnvelopePayload::ForecastState(s.clone()));assert!(render(&v).contains(&"FIRST FORECASTS".into()));s.state=ForecastStatus::Framing;s.state_version=0;v.apply(EventId::new(),&EnvelopePayload::ForecastState(s));assert_eq!(v.state.unwrap().state,ForecastStatus::Forecasting);
}
#[test]
fn pooled_bar_keeps_spread_band_and_individual_ticks(){
 let a=forecast::ForecastAggregate{probabilities_pct:vec![20,50,80],sorted_log_odds:vec![-1.3862943611198906,0.0,1.3862943611198906],k:0,trimmed_log_odds:vec![-1.3862943611198906,0.0,1.3862943611198906],l:0.0,p:0.5,s:1.3862943611198906,low:0.2,high:0.8,minimum_pct:20,maximum_pct:80,split:None};
 let line=bar(&a);assert!(line.contains('░'));assert!(line.contains('█'));assert!(line.contains('│'));assert!(line.starts_with("0%"));assert!(line.ends_with("100%"));
 let mut v=View::default();let s=state();v.apply(EventId::new(),&EnvelopePayload::ForecastState(s.clone()));v.apply(EventId::new(),&EnvelopePayload::ContestArtifact(ContestArtifact{contest_id:s.contest_id,step_id:StepId::new(),artifact_kind:Text::new("forecast.aggregate").unwrap(),kind:Some(ContestKind::Forecast),round:Some(0),actor:Some(Actor::System),audience:Some(ArtifactAudience::Human),body:Some(serde_json::to_value(a).unwrap()),data:serde_json::json!({}),work_id:None,source_refs:vec![]}));assert!(render(&v).iter().any(|s|s.contains("Pooled 50.0%")));
}
