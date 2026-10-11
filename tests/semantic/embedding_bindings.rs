//! Target admission and product wiring are separate from the pure vector codec.
use morphiecore::{
    adapter::embeddings::Request,
    provider::{AuthScheme, ProviderDefinition, ProviderId, TrustedOrigin},
    semantic::task::embedding::EmbeddingRequest,
};
#[path = "../support/embeddings.rs"]
mod fixed;

#[test]
fn preparation_uses_final_typed_encoding_without_mutating_source_or_injecting_generation_controls()
{
    let provider = ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse("http://127.0.0.1:9").unwrap(),
        chat_completions: None,
        responses: None,
        messages: None,
        auth: AuthScheme::Bearer,
    };
    let (_, route) = fixed::binding(&provider);
    let request = Request::new(
        "public-vector",
        EmbeddingRequest::new(vec!["synthetic text".into()]).unwrap(),
    );
    let original = request.clone();
    let secret = morphiecore::provider::SecretMaterial::new("synthetic-key").unwrap();
    let prepared =
        morphiecore::execution::embeddings::prepare(&route, &provider, &secret, &request).unwrap();
    assert_eq!(request, original);
    assert_eq!(prepared.method, "POST");
    assert_eq!(prepared.path, "/embeddings");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
        serde_json::json!({"model":"private-vector","input":["synthetic text"],"encoding_format":"float"})
    );
}
