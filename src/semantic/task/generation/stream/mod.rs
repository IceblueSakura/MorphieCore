//! Ordered Generation streams: event grammar, bounded state and materialization.
pub(in crate::semantic::task::generation) mod audio;
mod event;
mod reduce;
mod snapshot;
mod state;
pub(in crate::semantic::task::generation) mod value;

pub use event::{EventError, ItemKind, PartKind, StreamEvent, StreamTerminal};
pub use reduce::reduce;
pub use snapshot::{end_of_stream, materialize, snapshot_items};
pub use state::{StreamItem, StreamPart, StreamState};
