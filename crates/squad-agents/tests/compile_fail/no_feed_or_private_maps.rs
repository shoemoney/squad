fn inspect(context: squad_protocol::AgentContext) {
    let _ = context.submit.subscribe();
    let _ = context.work.private_map("labels");
}
fn main() {}
