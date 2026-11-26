use rusqlite::{Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn apply(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    updates: &[VariableUpdate],
) -> Result<()> {
    for update in updates {
        let max = match update.name {
            VariableName::Rounds => 20,
            VariableName::NegotiateRounds => 6,
            VariableName::ReadbackRounds => 4,
            VariableName::DebugRounds => 6,
            VariableName::TuneGenerations => 6,
            VariableName::PrdHumanReview => 1,

            VariableName::GrillRounds => 10,
            VariableName::CensusPasses => 5,
        };
        if (update.value == 0 && update.name != VariableName::PrdHumanReview) || update.value > max
        {
            return Err(StoreError::Domain(
                ErrorCode::INVALID_COMMAND,
                "Variable value is out of range",
            ));
        }
    }
    for update in updates {
        let name = update.name.as_str();
        let previous: u8 =
            tx.query_row("SELECT value FROM vars WHERE name=?", [name], |r| r.get(0))?;
        let seq = group.push(
            tx,
            super::envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::VarChange(VariableChange {
                    name: update.name.clone(),
                    value: update.value,
                    previous_value: previous,
                }),
            ),
        )?;
        tx.execute(
            "UPDATE vars SET value=?,updated_seq=? WHERE name=?",
            params![update.value, seq.get(), name],
        )?;
    }
    Ok(())
}
