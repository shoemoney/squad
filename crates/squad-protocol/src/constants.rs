//! Execution constants; shared with configuration without duplicating values.
pub use crate::limits::*;
pub const INGRESS_CAPACITY: usize = 256;
pub const FEED_CAPACITY: usize = 4_096;
pub const MAILBOX_CAPACITY: usize = 1;
pub const WORK_LEASE_MS: u64 = 60_000;
pub const WORK_LEASE_RENEW_MS: u64 = 20_000;
