// Module declarations for the app-server protocol namespace.
// Exposes protocol pieces used by `lib.rs` via `pub use protocol::common::*;`.

pub mod common;
pub mod event_mapping;
pub mod item_builders;
mod mappers;
mod serde_helpers;
mod task_service_model_projection;
pub mod thread_history;
pub mod thread_history_projection;
pub mod v1;
pub mod v2;
