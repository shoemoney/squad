//! Grill inputs come from the frozen setup and the work-authorized projection.
use super::{Inputs, PromptError, PromptPack, inputs::member_inputs};
use serde_json::json;
use squad_protocol::*;
/// Router-authorized grill record references: the compact list of questions
/// asked so far and the current round's shared floor. Each is delivered once.
#[derive(Debug, Clone, Default)]
pub struct GrillRecord {
    pub asked_questions: Vec<String>,
    pub floor: Vec<String>,
}
impl PromptPack {
    pub fn grill_inputs(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        frozen: &GrillPreflight,
        state: &GrillState,
    ) -> Result<(&'static str, Inputs), PromptError> {
        self.grill_inputs_with(
            config,
            work,
            binding,
            frozen,
            state,
            &GrillRecord::default(),
        )
    }

    pub fn grill_inputs_with(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        frozen: &GrillPreflight,
        state: &GrillState,
        record: &GrillRecord,
    ) -> Result<(&'static str, Inputs), PromptError> {
        let invalid =
            || PromptError::new("grill", None, "work differs from frozen grill assignment");
        frozen.setup.validate().map_err(|_| invalid())?;
        state.validate().map_err(|_| invalid())?;
        work.validate().map_err(|_| invalid())?;
        let setup = &frozen.setup;
        if work.contest_id != Some(setup.contest_id)
            || setup.contest_id != state.contest_id
            || work.round != Some(state.round)
            || state.total_rounds != setup.total_rounds
            || state.leader != setup.leader
        {
            return Err(invalid());
        }
        let (id, expected) = match work.kind {
            WorkKind::GrillQuestion if state.state == GrillStatus::Questioning => {
                ("grill.question", &state.griller_order[state.turn as usize])
            }
            WorkKind::GrillAnswer if state.state == GrillStatus::Answering => {
                ("grill.answer", &setup.leader)
            }
            WorkKind::GrillRebuild if state.state == GrillStatus::Rebuilding => {
                ("grill.rebuild", &setup.leader)
            }
            _ => return Err(invalid()),
        };
        if work.target != Target::Member(expected.clone()) {
            return Err(invalid());
        }
        let mut values = member_inputs(config, work, binding)?;
        values.extend(Inputs::from([
            ("contest_id".into(), json!(setup.contest_id)),
            ("round".into(), json!(state.round)),
            ("grill_rounds".into(), json!(setup.total_rounds)),
            ("leader_nick".into(), json!(setup.leader)),
            ("plan_version".into(), json!(state.plan_version)),
            ("next_plan_version".into(), json!(state.plan_version + 1)),
            (
                "grill_angle".into(),
                json!(crate::orchestrator::grill::angle(state)),
            ),
            ("grill_answer_word_limit".into(), json!(frozen.answer_words)),
            ("plan_word_limit".into(), json!(frozen.plan_words)),
            ("grill_focus_json".into(), json!(setup.focus)),
            ("grill_asked_json".into(), json!(record.asked_questions)),
            ("grill_floor_json".into(), json!(record.floor)),
        ]));
        Ok((id, self.selected(id, values, None)?))
    }
}
