//! MorphieCore semantic core, protocol codecs, target lowering, trusted
//! provider/topology bindings and a minimal execution chain.
//!
//! The gateway crate connects these pure layers to bounded HTTP I/O. The
//! optional-use binary loads explicit private credential files and listens only
//! on loopback; pure codec layers never read private configuration.

pub use morphiecore_catalog::{provider, topology};
pub use morphiecore_gateway as gateway;
pub use morphiecore_protocol as protocol;
pub use morphiecore_protocol::adapter;
pub use morphiecore_protocol::lowering;
pub use morphiecore_runtime::{account, configuration, credential, execution, transport};
pub use morphiecore_semantic as semantic;
