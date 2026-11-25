//! Bounded ingress and nonblocking coalesced wakeups. Durable state is the recovery queue.
use crate::router::{Router, rejected, request_id};
use squad_protocol::*;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, oneshot};
struct Ingress {
    request: Request,
    owner: Option<ExecutorKey>,
    reply: oneshot::Sender<RequestResult>,
}
type Mailboxes = Arc<Mutex<Vec<(ExecutorKey, mpsc::Sender<Wakeup>)>>>;
#[derive(Clone)]
pub struct InProcessBus {
    ingress: mpsc::Sender<Ingress>,
    feed: broadcast::Sender<FeedItem>,
    mailboxes: Mailboxes,
}
impl InProcessBus {
    pub fn start(mut router: Router) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Ingress>(constants::INGRESS_CAPACITY);
        let (feed, _) = broadcast::channel(constants::FEED_CAPACITY);
        let mailboxes: Mailboxes = Arc::new(Mutex::new(Vec::new()));
        let bus = Self {
            ingress: sender,
            feed: feed.clone(),
            mailboxes: mailboxes.clone(),
        };
        std::thread::Builder::new()
            .name("squad-router".into())
            .spawn(move || {
                while let Some(ingress) = receiver.blocking_recv() {
                    let id = request_id(&ingress.request);
                    super::diagnostics::request(id, ingress.owner.as_ref());
                    let ephemeral = match &ingress.request {
                        Request::Internal(InternalRequest::Executor(ExecutorRequest::Delta {
                            delta,
                            ..
                        })) => Some(EnvelopePayload::Delta(delta.clone())),
                        Request::Internal(InternalRequest::Executor(
                            ExecutorRequest::Presence(presence),
                        )) => Some(EnvelopePayload::Presence(presence.clone())),
                        _ => None,
                    };
                    match router.process(ingress.request, ingress.owner.clone()) {
                        Ok(outcome) => {
                            let notify = !outcome.duplicate && !outcome.events.is_empty();
                            // The router returns only after COMMIT. Failures cannot reach this path.
                            let _ = ingress.reply.send(outcome.result.clone());
                            for event in outcome.events {
                                super::diagnostics::commit(&event);
                                let _ = feed.send(FeedItem::Committed(event));
                            }
                            if let Some(content) = ephemeral
                                && matches!(outcome.result, RequestResult::Accepted { .. })
                            {
                                let sender = match ingress.owner {
                                    Some(ExecutorKey::Member(n)) => Actor::Member(n),
                                    _ => Actor::Judge,
                                };
                                let event = crate::router::envelope(
                                    id,
                                    sender,
                                    vec![Recipient::Me],
                                    content,
                                );
                                let item = if matches!(event.content, EnvelopePayload::Delta(_)) {
                                    FeedItem::Delta(event)
                                } else {
                                    FeedItem::Presence(event)
                                };
                                let _ = feed.send(item);
                            }
                            if notify {
                                let boxes = mailboxes.lock().expect("mailbox lock");
                                for (key, sender) in boxes.iter() {
                                    let result = sender.try_send(Wakeup);
                                    super::diagnostics::mailbox(key, result.is_err());
                                }
                            }
                        }
                        Err(error) => {
                            let _ = ingress.reply.send(rejected(id, error));
                        }
                    }
                }
            })
            .expect("router thread");
        bus
    }
    pub fn register(&self, key: ExecutorKey) -> mpsc::Receiver<Wakeup> {
        let (sender, receiver) = mpsc::channel(constants::MAILBOX_CAPACITY);
        let mut boxes = self.mailboxes.lock().expect("mailbox lock");
        boxes.retain(|(old, _)| *old != key);
        boxes.push((key, sender));
        receiver
    }
    pub fn executor_submit(&self, key: ExecutorKey) -> Arc<dyn ExecutorSubmit> {
        Arc::new(ScopedSubmit {
            bus: self.clone(),
            owner: key,
        })
    }
    async fn submit(&self, request: Request, owner: Option<ExecutorKey>) -> RequestResult {
        let id = request_id(&request);
        let (reply, receiver) = oneshot::channel();
        if self
            .ingress
            .send(Ingress {
                request,
                owner,
                reply,
            })
            .await
            .is_err()
        {
            return rejected(id, squad_store::StoreError::Unavailable);
        }
        receiver
            .await
            .unwrap_or_else(|_| rejected(id, squad_store::StoreError::Unavailable))
    }
}
impl Bus for InProcessBus {
    fn send(&self, request: Request) -> SubmitFuture<'_> {
        Box::pin(self.submit(request, None))
    }
    fn subscribe(&self) -> FeedReceiver {
        self.feed.subscribe()
    }
    fn wake_mailbox(&self, key: &ExecutorKey) {
        let boxes = self.mailboxes.lock().expect("mailbox lock");
        if let Some((_, sender)) = boxes.iter().find(|(k, _)| k == key) {
            let result = sender.try_send(Wakeup);
            super::diagnostics::mailbox(key, result.is_err());
        }
    }
}
struct ScopedSubmit {
    bus: InProcessBus,
    owner: ExecutorKey,
}
impl ExecutorSubmit for ScopedSubmit {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(self.bus.submit(
            Request::Internal(InternalRequest::Executor(request)),
            Some(self.owner.clone()),
        ))
    }
}
