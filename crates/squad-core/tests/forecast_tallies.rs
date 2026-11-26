use squad_core::orchestrator::forecast::*;
#[test]
fn hand_computed_aggregates(){
let a=aggregate(&[20, 50, 80]).unwrap();assert_eq!(a.k,0);assert!((a.l-(0.0)).abs()<1e-9);assert!((a.p-0.5).abs()<1e-9);assert!((a.s-1.3862943611198906).abs()<1e-9);
let a=aggregate(&[10, 20, 50, 80, 90]).unwrap();assert_eq!(a.k,1);assert!((a.l-(0.0)).abs()<1e-9);assert!((a.p-0.5).abs()<1e-9);assert!((a.s-1.837063948114373).abs()<1e-9);
let a=aggregate(&[5, 10, 20, 30, 40, 60, 70, 80, 90, 95]).unwrap();assert_eq!(a.k,2);assert!((a.l-(-9.25185853854297e-18)).abs()<1e-9);assert!((a.p-0.5).abs()<1e-9);assert!((a.s-1.903309656540869).abs()<1e-9);
}
#[test]
fn split_quorum_and_herding(){
assert!(aggregate(&[5,8,10,90,92,95]).unwrap().split.is_some());assert!(aggregate(&[40,45,50,55,60]).unwrap().split.is_none());assert!(aggregate(&[0,50,100]).is_err());assert!(aggregate(&[40,60]).is_err());assert!(uncited_move(10,40,0.0,&[]));assert!(!uncited_move(10,40,0.0,&["F1".into()]));
}
#[test]
fn withholding_patterns_and_word_boundaries(){
 let text="25% 25 percent 0.25 .75 1 in 4 2 to 3 3:7";
 let out=withhold(text);assert_eq!(out.matches("[number withheld]").count(),7);
 assert_eq!(withhold("Prototype launched in 2024"),"Prototype launched in 2024");
}
