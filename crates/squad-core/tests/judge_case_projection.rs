//! M1.P3.L3.T6 / M1.P3.L4.T8: judge-case turn projections and the pending
//! round-scoring allocation are persisted once at the numbered-round barrier;
//! fresh explicit-judge cases never share or duplicate a source projection.
mod support;
use squad_protocol::*;
use support::*;

async fn fixture() -> Fixture {
    Fixture::new(2, 20_000_000).await
}

fn rows(f: &Fixture, sql: String) -> Vec<(String, String)> {
    f.store
        .call(move |db| {
            let mut q = db.prepare(&sql)?;
            let rows = q
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .unwrap()
}

#[tokio::test]
async fn round_case_persists_projection_and_pending_round_score_allocation_once() {
    let f = fixture().await;
    let (work, case) = f.round_case();
    // Exactly one numbered-round scoring allocation exists, bound to the case,
    // and no score has been recorded yet (it is pending, not synthesised).
    let allocation = rows(
        &f,
        format!(
            "SELECT work_id, kind || '/' || COALESCE(round,'-') || '/' || case_id FROM work WHERE case_id='{case}'"
        ),
    );
    assert_eq!(allocation.len(), 1);
    assert_eq!(allocation[0].0, work.to_string());
    assert_eq!(allocation[0].1, format!("judge.round/1/{case}"));
    let scores: i64 = f
        .store
        .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM round_scores", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(scores, 0);
    // The turn projections landed in the case context at or before the work's
    // frozen boundary, one per (source event, projection key).
    let history = rows(
        &f,
        format!(
            "SELECT origin_event_id, projection_key FROM history WHERE context_id='judge:{case}' ORDER BY pos"
        ),
    );
    assert!(!history.is_empty());
    let mut unique = history.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        history.len(),
        "duplicate source projection: {history:?}"
    );
    let (boundary, last): (i64, i64) = f
        .store
        .call(move |db| {
            Ok((
                db.query_row(
                    "SELECT history_through FROM work WHERE work_id=?",
                    [work.to_string()],
                    |r| r.get(0),
                )?,
                db.query_row(
                    &format!("SELECT MAX(pos) FROM history WHERE context_id='judge:{case}'"),
                    [],
                    |r| r.get(0),
                )?,
            ))
        })
        .unwrap();
    assert_eq!(boundary, last, "allocation freezes the full projected case");
    let read = f
        .store
        .call(move |db| {
            squad_store::history::read(
                db,
                &ContextId(format!("judge:{case}")),
                HistoryPosition::new(boundary).unwrap(),
            )
        })
        .unwrap();
    assert_eq!(read.len(), history.len());
}

#[tokio::test]
async fn fresh_explicit_judge_cases_are_isolated_from_the_accumulating_debate_case() {
    let f = fixture().await;
    let (_, debate_case) = f.round_case();
    let (standalone_work, fresh_case) = f.standalone("FRESH");
    assert_ne!(debate_case, fresh_case);
    let contexts = rows(
        &f,
        "SELECT context_id, CAST(COUNT(*) AS TEXT) FROM history GROUP BY context_id ORDER BY context_id".into(),
    );
    let count = |ctx: String| -> usize {
        contexts
            .iter()
            .find(|(c, _)| *c == ctx)
            .map(|(_, n)| n.parse().unwrap())
            .unwrap_or(0)
    };
    assert!(count(format!("judge:{debate_case}")) > 0);
    assert!(count(format!("judge:{fresh_case}")) > 0);
    // The fresh case holds only its own question and quoted evidence.
    let fresh = rows(
        &f,
        format!("SELECT context_id, content FROM history WHERE context_id='judge:{fresh_case}'"),
    );
    let text = fresh
        .iter()
        .map(|r| r.1.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("QUESTION_FRESH") && text.contains("EVIDENCE_FRESH"));
    assert!(
        !text.contains("argument"),
        "debate transcript leaked into a fresh case"
    );
    // Source events projected into both cases are separate rows with distinct contexts.
    let shared = rows(
        &f,
        format!(
            "SELECT a.origin_event_id, a.projection_key FROM history a JOIN history b ON a.origin_event_id=b.origin_event_id AND a.projection_key=b.projection_key AND a.context_id<>b.context_id WHERE a.context_id IN ('judge:{debate_case}','judge:{fresh_case}')"
        ),
    );
    assert!(
        shared.is_empty(),
        "cases share a source projection: {shared:?}"
    );
    // The fresh case has its own single work item and no round-score allocation.
    assert_eq!(
        rows(
            &f,
            format!("SELECT work_id, kind FROM work WHERE case_id='{fresh_case}'")
        ),
        vec![(standalone_work.to_string(), "judge".to_string())]
    );
}
