use super::*;
use std::io::Write;
use tracing::instrument::WithSubscriber;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}
struct PassReader;
impl WorkReader for PassReader {
    fn prompt_values_for_work(&self, _: WorkId) -> ReaderFuture<'_, BTreeMap<String, String>> {
        Box::pin(async { Ok(BTreeMap::from([("census_pass".into(), "2".into())])) })
    }
    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        panic!("unused")
    }
    fn get(&self, _: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        panic!("unused")
    }
    fn repair_for_attempt(&self, _: WorkId, _: u32) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        panic!("unused")
    }
}

#[tokio::test]
async fn real_stream_attempts_emit_all_new_mode_identities_without_bodies() {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .without_time()
        .with_writer(capture.clone())
        .finish();
    let server = fake::FakeOpenRouter::start().await;
    server
        .completions(vec![fake::streaming("PRIVATE_OUTPUT", "stop"); 11])
        .await;
    let mut identities = Vec::new();
    async {
        for kind in [
            WorkKind::CensusHunt,
            WorkKind::CrosscheckAudit,
            WorkKind::SurveyExtract,
            WorkKind::ProbePredict,
            WorkKind::CensusMerge,
            WorkKind::CensusFinal,
            WorkKind::CrosscheckGoal,
            WorkKind::CrosscheckNormalize,
            WorkKind::CrosscheckRule,
            WorkKind::SurveyReport,
            WorkKind::ProbeReport,
        ] {
            let fixture = Fixture::new("alice", "PRIVATE_INPUT");
            let mut grant = fixture.grant.lock().unwrap().clone();
            grant.binding.effort = ReasoningEffort::High;
            let judge = kind.census_judge()
                || kind.is_crosscheck_judge()
                || matches!(
                    kind,
                    WorkKind::SurveyExtract | WorkKind::SurveyReport | WorkKind::ProbeReport
                );
            grant.work.kind = kind;
            if judge {
                let case = CaseId::new();
                grant.owner = ExecutorKey::Case(case);
                grant.work.target = Target::Judge;
                grant.work.case_id = Some(case);
            }
            grant.work.contest_id = Some(ContestId::new());
            grant.work.step_id = Some(StepId::new());
            grant.work.context_id = ContextId(format!("execution:{}", grant.work.work_id));
            identities.extend([
                grant.work.contest_id.unwrap().to_string(),
                grant.work.step_id.unwrap().to_string(),
                grant.work.context_id.0.clone(),
                grant.work.work_id.to_string(),
            ]);
            let (mut context, _) = fixture.context();
            context.work = Arc::new(PassReader);
            context.executor = grant.owner.clone();
            let mut request = executor(&server).request.clone();
            request.published_output_limit = 16384;
            request.messages = vec![ChatMessage {
                role: "user".into(),
                name: None,
                content: "PRIVATE_INPUT".into(),
            }];
            squad_agents::agents::stream_attempt(
                &context,
                &grant,
                &OpenRouterClient::new(&server.base_url(), "fixture-key").unwrap(),
                request,
                CancellationToken::new(),
                false,
            )
            .await
            .unwrap();
        }
    }
    .with_subscriber(subscriber)
    .await;
    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert_eq!(logs.matches("Provider attempt attributed").count(), 11);
    for identity in identities {
        assert!(logs.contains(&identity), "{identity}");
    }
    assert!(logs.contains("pass=Some(2)") && logs.contains("attempt=1"));
    for private in [
        "PRIVATE_INPUT",
        "PRIVATE_OUTPUT",
        "PRIVATE REASONING",
        "fixture-key",
    ] {
        assert!(!logs.contains(private), "leaked {private}");
    }
    let calls = server.captured_bodies().await;
    assert_eq!(calls.len(), 11);
    for (index, body) in calls.iter().enumerate() {
        // SurveyExtract and the final seven entries are structured judges.
        if index == 2 || index >= 4 {
            assert_eq!(body["reasoning"]["max_tokens"], 4096, "call {index}");
            assert_eq!(body["max_tokens"], 8192, "call {index}");
        }
    }
}
