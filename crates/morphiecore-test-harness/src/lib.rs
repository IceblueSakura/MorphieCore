//! Shared synthetic HTTP/router integration harnesses for MorphieCore root integration suites.

pub mod credential_recovery;
pub mod embedding_loop;
pub mod embeddings;
pub mod gateway;
pub mod google_tool_loop;
pub mod image_generation;
pub mod media_bindings;
pub mod messages_tool_loop;
pub mod openai_tool_loop;
pub mod resources;
pub mod semantic_events;
pub mod speech;
pub mod upstream_assets;

pub(crate) mod morphiecore {
    pub use morphiecore_catalog::{provider, topology};
    pub use morphiecore_gateway as gateway;
    pub use morphiecore_protocol::{self as protocol, adapter};
    pub use morphiecore_runtime::{account, credential, execution};
    pub use morphiecore_semantic as semantic;
}
