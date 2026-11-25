mod support;
use squad_protocol::*;
use support::*;

#[tokio::test]
async fn help_uses_local_result_without_durable_identity() {
    let room = FakeRoom::default();
    room.results
        .borrow_mut()
        .push_back(RequestResult::Internal(InternalResult::Text {
            text: "Shared parser help.".into(),
        }));
    let (code, output, _) = send(&room, &args("/help", true, false), None).await;
    assert_eq!(code, 0);
    assert!(!output.contains("cursor"));
    assert!(!output.contains("eventIds"));
    insta::assert_snapshot!(normalize(&output), @r#"{"requestId":"01900000-0000-7000-8000-000000000001","result":{"command":"help","text":"Shared parser help."},"type":"command.result"}"#);
}

#[tokio::test]
async fn shared_slash_mutations_use_shared_requests() {
    for message in [
        "/stop",
        "/stop @alpha",
        "/clear",
        "/set ROUNDS 3",
        "/set GRILL_ROUNDS 2",
        "/set CENSUS_PASSES 4",
    ] {
        let room = FakeRoom::default();
        let (code, _, _) = send(&room, &args(message, true, true), None).await;
        assert_eq!(code, 0, "{message}");
        assert_eq!(room.requests.borrow().len(), 1);
        assert!(!matches!(
            room.requests.borrow()[0],
            Request::UserMessage { .. }
        ));
    }
}

#[tokio::test]
async fn parser_errors_retain_the_exact_diagnostic() {
    let room = FakeRoom::default();
    for message in [
        "/quit",
        "/set ROUNDS 99",
        "/set CENSUS_PASSES 9",
        "@missing hello",
    ] {
        let error = squad_protocol::parse(message, &Default::default(), &room.context).unwrap_err();
        let (code, output, _) = send(&room, &args(message, true, false), None).await;
        assert_eq!(code, 1);
        let record: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(record["error"]["message"], error.message);
        assert_eq!(
            record["error"]["code"],
            serde_json::to_value(error.code).unwrap()
        );
    }
    assert!(room.requests.borrow().is_empty());
    assert_eq!(room.shutdowns.get(), 4);
}

// ---- M3.P3.L2.T4: judge, regeneration, export and mode-continuation commands ----

#[tokio::test]
async fn judge_and_regen_commands_carry_their_identifiers_into_shared_requests() {
    let debate = ID;
    let message = "01900000-0000-7000-8000-000000000002";
    for (text, check) in [
        ("/judge", 0u8),
        ("/judge 01900000-0000-7000-8000-000000000001", 1),
        ("/regen", 2),
        ("/regen 01900000-0000-7000-8000-000000000002", 3),
    ] {
        let room = FakeRoom::default();
        let (code, output, _) = send(&room, &args(text, true, true), None).await;
        assert_eq!(code, 0, "{text}");
        assert!(output.contains("\"type\":\"ack\""));
        let requests = room.requests.borrow();
        match (&requests[0], check) {
            (
                Request::Judge {
                    debate_id: None, ..
                },
                0,
            )
            | (
                Request::Regen {
                    message_id: None, ..
                },
                2,
            ) => {}
            (
                Request::Judge {
                    debate_id: Some(id),
                    ..
                },
                1,
            ) => assert_eq!(id.to_string(), debate),
            (
                Request::Regen {
                    message_id: Some(id),
                    ..
                },
                3,
            ) => assert_eq!(id.to_string(), message),
            (other, _) => panic!("{text}: {other:?}"),
        }
    }
}

#[tokio::test]
async fn command_usage_errors_keep_the_shared_wording_and_never_reach_the_room() {
    let room = FakeRoom::default();
    for message in [
        "/judge one two",
        "/judge not-an-id",
        "/regen one two",
        "/regen nope",
        "/export",
        "/export room 01900000-0000-7000-8000-000000000001",
        "/export nowhere",
        "/continue not-json",
        "/accept -- reason",
        "/stop @missing",
    ] {
        let error = squad_protocol::parse(message, &Default::default(), &room.context).unwrap_err();
        let (code, output, _) = send(&room, &args(message, true, false), None).await;
        assert_eq!(code, 1, "{message}");
        let record: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(record["error"]["message"], error.message, "{message}");
    }
    assert!(room.requests.borrow().is_empty());
}

#[tokio::test]
async fn continue_and_accept_without_an_active_mode_context_are_refused_before_the_room() {
    let room = FakeRoom::default();
    for message in [
        "/continue",
        "/accept 01900000-0000-7000-8000-000000000009 -- the risk is acceptable",
    ] {
        let (code, output, _) = send(&room, &args(message, true, false), None).await;
        assert_eq!(code, 1, "{message}: {output}");
        let record: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert!(
            record["error"]["message"]
                .as_str()
                .is_some_and(|m| !m.is_empty())
        );
    }
    assert!(room.requests.borrow().is_empty());
}

#[tokio::test]
async fn export_commands_report_ready_files_with_scope_and_id_without_touching_the_room() {
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    let contest = "01900000-0000-7000-8000-000000000042";
    let mut seen = Vec::new();
    for (text, scope) in [
        ("/export room".to_owned(), "room"),
        (format!("/export debate {contest}"), "debate"),
        (format!("/export tournament {contest}"), "tournament"),
        (format!("/export grill {contest}"), "grill"),
        (format!("/export census {contest}"), "census"),
        (format!("/export crosscheck {contest}"), "crosscheck"),
        (format!("/export survey {contest}"), "survey"),
        (format!("/export probe {contest}"), "probe"),
    ] {
        let room = FakeRoom::default();
        let (code, output, _) = send(&room, &args(&text, true, false), None).await;
        assert_eq!(code, 0, "{text}");
        let record: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(record["type"], "export.ready");
        assert_eq!(record["scope"], scope);
        if scope == "room" {
            assert!(record.get("id").is_none());
            assert_eq!(record["filename"], "squad-room.md");
        } else {
            assert_eq!(record["id"], contest);
            assert_eq!(record["filename"], format!("squad-{scope}-{contest}.md"));
        }
        assert!(
            room.requests.borrow().is_empty(),
            "export is not a room mutation"
        );
        seen.push(record["filename"].as_str().unwrap().to_owned());
    }
    std::env::set_current_dir(previous).unwrap();
    for name in seen {
        assert!(temp.path().join(name).is_file());
    }
}
