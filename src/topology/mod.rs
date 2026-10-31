//! Compiled endpoint topology: trusted provider bindings, fixed routes and
//! public model contracts.
//!
//! A business request resolves only a public model label. It can never select an
//! origin, path, credential or protocol profile; those relations are fixed at
//! compile time and immutable afterwards.

pub mod catalog;
pub mod compile;
pub mod embeddings;
pub mod endpoint;
pub mod images;
pub mod model;
mod model_metadata;
pub mod route;
pub mod speech;
pub mod transcription;

pub use compile::{CompiledTopology, TopologyError, compile};
pub use endpoint::{Endpoint, EndpointTarget, ExecutionContract};
pub use model::{CanonicalModel, Dispositions};
pub use model_metadata::ModelMetadata;
pub use route::{CandidatePolicy, FallbackPolicy, PublicModel, Route, RoutePolicy};

use crate::provider::{ProviderError, ident_ok};

macro_rules! ident {
    ($name:ident, $valid:expr) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(id: &str) -> Result<Self, ProviderError> {
                if ($valid)(id) {
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

ident!(ModelId, |id: &str| {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
});
ident!(EndpointId, ident_ok);
ident!(RouteId, ident_ok);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    Generation,
}

/// Wire protocol profile an endpoint speaks. Kept separate from codec types so
/// topology stays free of protocol implementation internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationProtocol {
    OpenAiChat,
    OpenAiResponses,
    AnthropicMessages,
    GoogleGenerateContent,
    GoogleInteractions,
}
impl GenerationProtocol {
    pub fn operation(self) -> crate::provider::GenerationOperation {
        use crate::provider::GenerationOperation;
        match self {
            Self::OpenAiChat => GenerationOperation::ChatCompletions,
            Self::OpenAiResponses => GenerationOperation::Responses,
            Self::AnthropicMessages => GenerationOperation::AnthropicMessages,
            Self::GoogleGenerateContent => GenerationOperation::GoogleGenerateContent,
            Self::GoogleInteractions => GenerationOperation::GoogleInteractions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ModelId;

    #[test]
    fn model_labels_do_not_encode_provider_selection() {
        assert_eq!(
            ModelId::new("Synthetic-Model").unwrap().as_str(),
            "Synthetic-Model"
        );
        assert!(ModelId::new("synthetic-provider/synthetic-model").is_err());
        assert!(super::EndpointId::new("Synthetic-Endpoint").is_err());
        assert!(super::RouteId::new("Synthetic-Route").is_err());
    }
}
