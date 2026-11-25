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
