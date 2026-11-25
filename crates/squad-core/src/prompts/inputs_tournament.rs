//! Tournament-only input mappings never accept room history or label maps.
use super::{Inputs, PromptError, PromptPack};
use serde_json::json;
use squad_protocol::*;
impl PromptPack {
    pub fn tournament_judge_inputs(
        &self,
        material: &TournamentCaseMaterial,
    ) -> Result<Inputs, PromptError> {
        Ok(Inputs::from([
            (
                "tournament_schema_json".into(),
                validation::schema::<TournamentJudgeResponse>(),
            ),
            ("tournament_judge_data_json".into(), json!(material)),
        ]))
    }
    pub fn tournament_entry_values(&self, state: &TournamentState, words: u16) -> Inputs {
        Inputs::from([
            ("contest_id".into(), json!(state.contest_id)),
            ("round".into(), json!(state.round)),
            ("entrants_remaining".into(), json!(state.survivors.len())),
            ("room_word_limit".into(), json!(words)),
        ])
    }
    pub fn tournament_result(&self, result: &TournamentResult) -> Result<String, PromptError> {
        self.render_template("tournament.result", &Inputs::from([("contest_id".into(),json!(result.contest_id)),("round".into(),json!(result.round)),("tournament_result_json".into(),json!({"rank":result.rank,"remainingBefore":result.remaining_before,"score":result.score,"critique":result.critique,"survived":result.survived}))]))
    }
    pub fn tournament_final(&self, champion: &TournamentChampion) -> Result<String, PromptError> {
        self.render_template(
            "tournament.final",
            &Inputs::from([
                ("contest_id".into(), json!(champion.contest_id)),
                (
                    "tournament_placements_json".into(),
                    json!(champion.placements),
                ),
            ]),
        )
    }
}
