//! Compiled endpoint topology: trusted provider bindings, fixed routes and
//! public model contracts.
//!
//! A business request resolves only a public model label. It can never select an
//! origin, path, credential or protocol profile; those relations are fixed at
//! compile time and immutable afterwards.

pub mod catalog;
pub mod compile;
pub mod endpoint;
pub mod images;
pub mod model;
mod model_metadata;
pub mod route;
pub mod speech;
pub mod transcription;

pub use compile::{CompiledTopology, TopologyError, compile};
pub use endpoint::{Endpoint, EndpointTarget, ExecutionContract};
pub use model::{CanonicalModel, GenerationSemanticContract};
pub use model_metadata::ModelMetadata;
pub use route::{CandidatePolicy, FallbackPolicy, PublicModel, Route, RoutePolicy};

use crate::provider::{ProviderError, ident_ok};

macro_rules! ident {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: &str) -> Result<Self, ProviderError> {
                if ident_ok(id) {
                    Ok(Self(id.into()))
                } else {
                    Err(ProviderError::InvalidId)
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

ident!(ModelId);
ident!(EndpointId);
ident!(RouteId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    Generation,
}

/// Wire protocol profile an endpoint speaks. Kept separate from codec types so
/// topology stays free of protocol implementation internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolProfile {
    OpenAiChat,
    OpenAiResponses,
}

#[cfg(test)]
mod tests {
    use super::ModelId;

    #[test]
    fn model_labels_do_not_encode_provider_selection() {
        assert_eq!(
            ModelId::new("synthetic-model").unwrap().as_str(),
            "synthetic-model"
        );
        assert!(ModelId::new("synthetic-provider/synthetic-model").is_err());
    }
}
