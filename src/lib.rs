//! MorphieCore semantic core, protocol codecs, target lowering, trusted
//! provider/topology bindings and a minimal execution chain.
//!
//! The gateway module connects these pure layers to bounded HTTP I/O. The
//! optional-use binary loads explicit private credential files and listens only
//! on loopback; pure codec layers never read private configuration.

pub mod account;
pub use protocol::adapter;
pub mod configuration;
pub mod credential;
pub mod execution;
pub mod gateway;
pub mod lowering;
pub mod protocol;
pub mod provider;
pub use morphiecore_semantic as semantic;
pub mod topology;
pub mod transport;
