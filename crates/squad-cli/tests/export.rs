mod support;
use serde_json::json;
use squad_cli::args::Export;
use squad_protocol::*;
use support::*;

#[test]
fn offline_export_reads_committed_content_while_room_is_locked() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![reply(1, "Keep the answer.")]);
    let _lock = squad_store::room_lock::RoomLock::acquire(&temp.path().join("main.lock")).unwrap();
    let options = Export {
        scope: ExportScope::Room,
        id: None,
        output: None,
    };
    let mut output = Vec::new();
    squad_cli::export::run(temp.path(), &options, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("Keep the answer."));
    assert!(text.contains("alpha"));
    insta::assert_snapshot!(text);
    let readonly = squad_cli::export::open_readonly(temp.path()).unwrap();
    assert!(readonly.execute("DELETE FROM events", []).is_err());
}

#[test]
fn output_file_contains_markdown_and_stdout_only_contains_path() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![reply(1, "Answer")]);
    let path = temp.path().join("output.md");
    let options = Export {
        scope: ExportScope::Room,
        id: None,
        output: Some(path.clone()),
    };
    let mut output = Vec::new();
    squad_cli::export::run(temp.path(), &options, &mut output).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap().trim(),
        path.to_str().unwrap()
    );
    assert!(std::fs::read_to_string(path).unwrap().contains("Answer"));
}

fn debate_state(n: u32, status: &str, round: u32) -> CommittedEnvelope {
    event(
        n,
        "debate.state",
        json!({"debateId":ID,"state":status,"debaters":["alpha","beta"],"round":round,"totalRounds":3,"judgeModel":"judge/model","judgeRequested":false,"stateVersion":n,"terminalReason":null,"caseId":null,"caseIds":[]}),
    )
}
#[test]
fn offline_debate_uses_fixed_opening_round_final_and_verdict_banners() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(
        temp.path(),
        vec![
            debate_state(1, "ROUND_RUNNING", 0),
            debate_state(2, "ROUND_RUNNING", 1),
            debate_state(3, "ROUND_RUNNING", 3),
            debate_state(4, "JUDGING", 3),
        ],
    );
    let options = Export {
        scope: ExportScope::Debate,
        id: Some(ID.parse().unwrap()),
        output: None,
    };
    let mut output = Vec::new();
    squad_cli::export::run(temp.path(), &options, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    for title in ["OPENING", "ROUND 1", "FINAL ROUND", "VERDICT"] {
        assert!(
            text.contains(&squad_core::banner::render_default(title)),
            "{title}: {text}"
        );
    }
    insta::assert_snapshot!(text);
}

#[test]
fn every_scope_delegates_to_core_and_preserves_errors() {
    let temp = tempfile::tempdir().unwrap();
    let _db = database(temp.path(), vec![]);
    for scope in [
        ExportScope::Debate,
        ExportScope::Tournament,
        ExportScope::Grill,
        ExportScope::Census,
        ExportScope::Crosscheck,
        ExportScope::Survey,
        ExportScope::Probe,
    ] {
        let options = Export {
            scope: scope.clone(),
            id: None,
            output: None,
        };
        assert_eq!(
            squad_cli::export::run(temp.path(), &options, &mut Vec::new()).unwrap_err(),
            "contest export requires an ID"
        );
        let options = Export {
            scope,
            id: Some(ID.parse().unwrap()),
            output: None,
        };
        assert!(squad_cli::export::run(temp.path(), &options, &mut Vec::new()).is_err());
    }
}
