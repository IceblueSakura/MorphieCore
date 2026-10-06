//! Publication facts belong to canonical identity, not an inference Provider or task IR.
use super::{CompiledTopology, ModelId, TopologyError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelMetadata {
    released_at: u64,
    developer: String,
}
impl ModelMetadata {
    /// Unix seconds of model publication; date-only sources use UTC midnight.
    pub fn new(released_at: u64, developer: &str) -> Result<Self, TopologyError> {
        if released_at == 0
            || released_at > i64::MAX as u64
            || developer.is_empty()
            || developer.len() > 128
            || developer.trim() != developer
            || developer.chars().any(char::is_control)
        {
            return Err(TopologyError::InvalidModelMetadata);
        }
        Ok(Self {
            released_at,
            developer: developer.into(),
        })
    }
    pub fn released_at(&self) -> u64 {
        self.released_at
    }
    pub fn developer(&self) -> &str {
        &self.developer
    }
}
impl CompiledTopology {
    pub fn model_metadata(&self, canonical: &ModelId) -> Option<&ModelMetadata> {
        self.model_metadata.get(canonical.as_str())
    }
    /// Attach explicit trusted facts once, after compiling task identities.
    /// Pure topology consumers may omit them; HTTP activation requires every selected identity.
    pub fn with_model_metadata(
        mut self,
        facts: impl IntoIterator<Item = (ModelId, ModelMetadata)>,
    ) -> Result<Self, TopologyError> {
        if !self.model_metadata.is_empty() {
            return Err(TopologyError::DuplicateModelMetadata);
        }
        for (id, metadata) in facts {
            if self.canonical_model(&id).is_none()
                && !self.image_routes.values().any(|r| r.canonical_model == id)
                && !self.speech_routes.values().any(|r| r.canonical_model == id)
                && !self
                    .transcription_routes
                    .values()
                    .any(|r| r.canonical_model == id)
            {
                return Err(TopologyError::UnknownCanonicalModel);
            }
            if self.model_metadata.len() >= 64 {
                return Err(TopologyError::InvalidModelMetadata);
            }
            if self
                .model_metadata
                .insert(id.as_str().into(), metadata)
                .is_some()
            {
                return Err(TopologyError::DuplicateModelMetadata);
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_metadata_preserves_values_and_rejects_invalid_facts() {
        assert!(ModelMetadata::new(i64::MAX as u64, "Developer").is_ok());
        let metadata = ModelMetadata::new(1_800_000_000, "Synthetic \"Developer\"").unwrap();
        assert_eq!(metadata.released_at(), 1_800_000_000);
        assert_eq!(metadata.developer(), "Synthetic \"Developer\"");
        for (time, owner) in [
            (0, "Developer"),
            (u64::MAX, "Developer"),
            (1, ""),
            (1, " "),
            (1, " Developer"),
            (1, "Developer "),
            (1, "Developer\nName"),
        ] {
            assert!(ModelMetadata::new(time, owner).is_err());
        }
        assert!(ModelMetadata::new(1, &"x".repeat(128)).is_ok());
        assert!(ModelMetadata::new(1, &"x".repeat(129)).is_err());
    }

    fn topology(count: usize) -> CompiledTopology {
        super::super::compile(
            vec![],
            vec![],
            vec![],
            vec![],
            (0..count)
                .map(|i| super::super::CanonicalModel {
                    id: ModelId::new(&format!("canonical-{i}")).unwrap(),
                    task: super::super::TaskKind::Generation,
                    contract: super::super::GenerationSemanticContract::full(),
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn metadata_identity_uniqueness_and_count_are_checked_without_a_gateway() {
        let fact = |i| {
            (
                ModelId::new(&format!("canonical-{i}")).unwrap(),
                ModelMetadata::new(1, "Synthetic Developer").unwrap(),
            )
        };
        assert_eq!(
            topology(0).with_model_metadata([fact(0)]).unwrap_err(),
            TopologyError::UnknownCanonicalModel
        );
        assert_eq!(
            topology(1)
                .with_model_metadata([fact(0), fact(0)])
                .unwrap_err(),
            TopologyError::DuplicateModelMetadata
        );
        let compiled = topology(1).with_model_metadata([fact(0)]).unwrap();
        assert_eq!(compiled.model_metadata(&fact(0).0), Some(&fact(0).1));
        assert_eq!(
            compiled.with_model_metadata([fact(0)]).unwrap_err(),
            TopologyError::DuplicateModelMetadata
        );
        assert!(topology(65).with_model_metadata((0..64).map(fact)).is_ok());
        assert_eq!(
            topology(65)
                .with_model_metadata((0..65).map(fact))
                .unwrap_err(),
            TopologyError::InvalidModelMetadata
        );
    }
}
