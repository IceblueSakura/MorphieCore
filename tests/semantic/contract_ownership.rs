//! Semantic admission must not depend on a wire profile or Provider binding.
use morphiecore::semantic::{
    task::generation::*,
    value::{Presence, Text},
};
#[test]
fn history_probability_presence_does_not_request_new_probability_generation() {
    for probabilities in [
        Presence::Null,
        Presence::Value(vec![]),
        Presence::Value(vec![Logprob {
            token: "synthetic".into(),
            logprob: serde_json::Number::from_f64(-0.5).unwrap(),
            bytes: None,
            top_logprobs: None,
        }]),
    ] {
        let request = GenerationRequest::new(
            vec![(
                ItemId::new(1),
                Item::Message(Message {
                    role: MessageRole::Assistant,
                    parts: vec![Part {
                        id: PartId::new(1),
                        content: ContentPart::Text(
                            TextContent::new(
                                Text::new("synthetic", "synthetic", 32).unwrap(),
                                vec![],
                                probabilities.clone(),
                            )
                            .unwrap(),
                        ),
                    }],
                    status: ItemLifecycle::Completed,
                    phase: None,
                }),
            )],
            GenerationControls::default(),
        )
        .unwrap();
        let mut contract = GenerationSemanticContract::text_images();
        contract.logprobs = false;
        assert!(contract.check(&request).is_ok());
        let q = GenerationRequirements::derive(&request);
        assert!(!q.logprobs);
        assert!(q.text_metadata);
        let response =
            GenerationResponse::new(request.items().to_vec(), Outcome::Completed).unwrap();
        assert!(GenerationResponseRequirements::derive(&response).logprobs);
        let mut settings = request.settings().clone();
        settings.controls.logprobs = Presence::Value(true);
        let controlled = request.clone().with_settings(settings).unwrap();
        assert_eq!(
            contract.check(&controlled).unwrap_err(),
            GenerationFeature::Controls
        );
        contract.text_metadata = false;
        assert_eq!(
            contract.check(&request).unwrap_err(),
            GenerationFeature::TextMetadata
        );
    }
}

#[test]
fn semantic_contract_checks_final_request_without_a_representation_contract() {
    let request = GenerationRequest::new(
        vec![(
            ItemId::new(1),
            Item::Message(Message {
                role: MessageRole::User,
                parts: vec![Part {
                    id: PartId::new(1),
                    content: ContentPart::Resource(Resource {
                        location: ResourceLocation::Url(
                            Text::new("https://example.invalid/a", "synthetic", 128).unwrap(),
                        ),
                        description: ResourceDescription::Image { detail: None },
                    }),
                }],
                status: ItemLifecycle::Completed,
                phase: None,
            }),
        )],
        GenerationControls::default(),
    )
    .unwrap();
    let mut contract = GenerationSemanticContract::text_images();
    assert!(contract.check(&request).is_ok());
    contract.image_input = false;
    assert_eq!(
        contract.check(&request).unwrap_err(),
        GenerationFeature::ImageInput
    );
}
