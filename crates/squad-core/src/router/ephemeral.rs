use squad_protocol::*;
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Ephemeral {
    chunks: BTreeMap<(WorkId, u32), u64>,
    pub presence: BTreeMap<String, Presence>,
}
impl Ephemeral {
    pub fn chunk(&mut self, delta: &Delta) -> bool {
        let key = (delta.generation.work_id, delta.generation.attempt);
        if self
            .chunks
            .get(&key)
            .is_some_and(|index| *index >= delta.chunk_index)
        {
            return false;
        }
        self.chunks.insert(key, delta.chunk_index);
        true
    }
}
