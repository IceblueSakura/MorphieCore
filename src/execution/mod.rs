//! Minimal execution chain: immutable plans, monotonic delivery lifecycle and
//! attempt preparation/intake over semantically blind transport edges.
//!
//! Execution consumes compiled topology. It does not interpret model semantics,
//! cannot create or modify topology relations, and never retries or falls back
//! after the first visible semantic output.
//!
//! Network I/O stays with the caller: this module exchanges typed request parts
//! and byte chunks so tests and the authorized probe entry drive the same chain.

pub mod attempt;
pub mod delivery;
pub mod fallback;
pub mod images;
pub mod lifecycle;
pub mod plan;
pub mod speech;
pub mod transcription;

pub use attempt::{Attempt, AttemptError, UpstreamRequest, admit, prepare};
pub use delivery::ResponseDelivery;
pub use lifecycle::{DeliveryState, Lifecycle, LifecycleError};
pub use plan::{Delivery, ExecutionPlan, PlanError};
