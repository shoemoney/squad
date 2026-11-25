use serde_json::json;
use squad_protocol::*;
#[test]
fn provider_cannot_accept_risk_or_supply_authoritative_fields() {
    let id = CellId::new();
    for status in ["accepted", "probably_covered"] {
        assert!(validation::decode::<CrosscheckReviewResponse>(&json!({"cells":[{"cellId":id,"status":status,"citation":null,"reason":"a reason","proposedFix":null}]})).is_err());
    }
    assert!(
        validation::decode::<CrosscheckRuling>(
            &json!({"cells":[],"additions":[],"residualRisks":[],"boundedStop":true})
        )
        .is_err()
    );
    assert!(
        validation::decode::<CrosscheckRuling>(
            &json!({"cells":[],"additions":[],"residualRisks":[],"acceptances":[]})
        )
        .is_err()
    );
}
#[test]
fn review_requires_exact_coverage_nullable_fields_and_real_citation() {
    let id = CellId::new();
    let good = json!({"cells":[{"cellId":id,"status":"covered","citation":{"location":"section 1","quote":"roll back"},"reason":"Handles the scenario","proposedFix":null}]});
    let response = validation::decode::<CrosscheckReviewResponse>(&good).unwrap();
    assert!(response.validate(&[id], "We roll back on failure.").is_ok());
    assert!(response.validate(&[id], "No recovery documented.").is_err());
    assert!(
        response
            .validate(&[CellId::new()], "We roll back on failure.")
            .is_err()
    );
    let mut missing = good;
    missing["cells"][0]
        .as_object_mut()
        .unwrap()
        .remove("proposedFix");
    assert!(validation::decode::<CrosscheckReviewResponse>(&missing).is_err());
    let duplicate = CrosscheckReviewResponse {
        cells: vec![response.cells[0].clone(), response.cells[0].clone()],
    };
    assert!(duplicate.validate(&[id, id], "roll back").is_err());
}
#[test]
fn inventory_is_three_items_and_denies_identity_metadata() {
    let good = json!({"items":(0..3).map(|i|json!({"localId":format!("i{i}"),"item":"A concrete case","rationale":"A reason"})).collect::<Vec<_>>()});
    let inventory = validation::decode::<CrosscheckInventoryResponse>(&good).unwrap();
    assert!(inventory.validate().is_ok());
    let mut wrong = good.clone();
    wrong["items"][0]["reviewer"] = json!("member0");
    assert!(validation::decode::<CrosscheckInventoryResponse>(&wrong).is_err());
    wrong = good;
    wrong["items"].as_array_mut().unwrap().pop();
    assert!(
        validation::decode::<CrosscheckInventoryResponse>(&wrong)
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn six_member_preflight_is_16_or_17_calls() {
    for extraction in [false, true] {
        let e = u32::from(extraction);
        let p = CrosscheckPreflight {
            base: ContestPreflight {
                contest_id: ContestId::new(),
                kind: ContestKind::Crosscheck,
                plan_id: PlanId::new(),
                expires_at: chrono::Utc::now(),
                estimate: DebateEstimate {
                    estimated_usd: 1.,
                    reserved_usd: 2.,
                    remaining_usd: 10.,
                    model_calls: 16 + e,
                },
                participants: (0..6)
                    .map(|i| MemberNick::new(&format!("member{i}")).unwrap())
                    .collect(),
                confirmation_required: false,
                warning: None,
                settings: json!({}),
            },
            extraction_required: extraction,
            reserved_attempts: 34 + 3 * e,
        };
        assert!(p.validate().is_ok());
        let mut invalid = p;
        invalid.reserved_attempts -= 1;
        assert!(invalid.validate().is_err());
    }
}
#[test]
fn instantiated_schema_constrains_this_work_ids_and_cardinality() {
    let id = CellId::new();
    let material = CrosscheckCaseMaterial::Review {
        plan: Text::new("A plan").unwrap(),
        cells: vec![CrosscheckVisibleCell {
            cell_id: id,
            items: vec![Text::new("A case").unwrap()],
        }],
    };
    let schema = material.response_schema();
    assert_eq!(schema["properties"]["cells"]["minItems"], 1);
    assert_eq!(schema["properties"]["cells"]["maxItems"], 1);
    assert_eq!(
        schema["$defs"]["CrosscheckCellReview"]["properties"]["cellId"]["enum"],
        json!([id])
    );
    let inventory = CrosscheckCaseMaterial::Inventory {
        goal: CrosscheckGoal {
            goal: Text::new("A goal").unwrap(),
            constraints: vec![],
            uncertainties: vec![],
        },
        lens: CrosscheckLens::Stakeholders,
    }
    .response_schema();
    assert_eq!(inventory["properties"]["items"]["minItems"], 3);
    assert_eq!(inventory["properties"]["items"]["maxItems"], 3);
}
#[test]
fn all_six_work_kinds_use_the_canonical_wire_names_and_roles() {
    for (kind, wire, judge) in [
        (WorkKind::CrosscheckGoal, "crosscheck.goal", true),
        (WorkKind::CrosscheckInventory, "crosscheck.inventory", false),
        (WorkKind::CrosscheckNormalize, "crosscheck.normalize", true),
        (WorkKind::CrosscheckCell, "crosscheck.cell", false),
        (WorkKind::CrosscheckAudit, "crosscheck.audit", false),
        (WorkKind::CrosscheckRule, "crosscheck.rule", true),
    ] {
        assert_eq!(serde_json::to_value(&kind).unwrap(), wire);
        assert_eq!(kind.is_crosscheck_judge(), judge);
        assert_eq!(kind.is_crosscheck_member(), !judge);
        let contest = ContestId::new();
        let step = StepId::new();
        let case = CaseId::new();
        let mut work = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind,
            target: if judge {
                Target::Judge
            } else {
                Target::Member("member0".parse().unwrap())
            },
            context_id: ContextId(if judge {
                format!("judge:{case}")
            } else {
                format!("contest:{contest}:{step}:member0")
            }),
            trigger_event_id: EventId::new(),
            history_through: HistoryPosition::new(1).unwrap(),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: judge.then_some(case),
            prompt_ref: None,
            max_output_tokens: Some(if judge { 4096 } else { 2048 }),
            contest_id: Some(contest),
            step_id: Some(step),
        };
        assert!(work.validate().is_ok());
        if !judge {
            work.context_id = ContextId("member0".into());
            assert!(work.validate().is_err());
        }
    }
}
