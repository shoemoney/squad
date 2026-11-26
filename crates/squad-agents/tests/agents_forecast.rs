use serde_json::json;
use squad_protocol::{forecast::validate_output,WorkKind};
#[test]
fn repair_diagnostics_reject_endpoints_then_accept_valid_revision(){
 let input=json!({"labels":["F1"],"wordLimit":200});
 for pct in [0,100]{assert!(validate_output(&WorkKind::ForecastRevise,&input,&json!({"probabilityPct":pct,"changeReason":"Evidence","citedLabels":["F1"]})).is_err());}
 validate_output(&WorkKind::ForecastRevise,&input,&json!({"probabilityPct":50,"changeReason":"Evidence","citedLabels":["F1"]})).unwrap();
 assert!(validate_output(&WorkKind::ForecastRevise,&input,&json!({"probabilityPct":50,"changeReason":"Evidence","citedLabels":["member0"]})).is_err());
}
#[test]
fn prose_budget_excludes_ids_but_rejects_oversized_reasons(){
 let input=json!({"labels":["F1"],"wordLimit":1});let mut output=json!({"probabilityPct":50,"changeReason":"Evidence","citedLabels":["F1"]});validate_output(&WorkKind::ForecastRevise,&input,&output).unwrap();output["changeReason"]=json!("Too many words");assert!(validate_output(&WorkKind::ForecastRevise,&input,&output).is_err());
}
