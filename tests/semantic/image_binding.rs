use morphiecore::{
    execution::images::prepare,
    gateway::{Credentials, Gateway, ImageEntry, Limits},
    provider::{
        AuthScheme, CredentialBindingId, ProviderDefinition, ProviderId, SecretMaterial,
        TrustedOrigin,
    },
    topology::{self, CompiledTopology, ModelId, TopologyError},
};
use std::sync::Arc;
const ORIGIN: &str = "https://synthetic.invalid";
fn provider() -> ProviderDefinition {
    ProviderDefinition {
        id: ProviderId::new("fixture").unwrap(),
        origin: TrustedOrigin::parse(ORIGIN).unwrap(),
        chat_completions: None,
        responses: None,
        auth: AuthScheme::Bearer,
    }
}
fn topology() -> CompiledTopology {
    topology::compile(vec![provider()], vec![], vec![], vec![], vec![]).unwrap()
}
#[test]
fn image_routes_require_explicit_operation_identity_and_bounded_execution() {
    let (operation, route) = super::image_support::binding(ORIGIN);
    assert!(topology().with_images(vec![], vec![route.clone()]).is_err());
    for case in 0..7 {
        let mut route = route.clone();
        match case {
            0 => {
                route.endpoint.target.origin =
                    TrustedOrigin::parse("https://other.invalid").unwrap()
            }
            1 => route.endpoint.canonical_model = ModelId::new("other").unwrap(),
            2 => route.endpoint.execution.streaming = true,
            3 => route.endpoint.execution.retry_before_commit = true,
            4 => route.endpoint.execution.response_body_limit = 0,
            5 => route.endpoint.upstream_model = "".into(),
            _ => {
                route.endpoint.target.path =
                    morphiecore::provider::EndpointPath::new("/other").unwrap()
            }
        }
        assert!(
            topology()
                .with_images(vec![operation.clone()], vec![route])
                .is_err()
        );
    }
    assert_eq!(
        topology()
            .with_images(vec![operation.clone()], vec![route.clone(), route.clone()])
            .unwrap_err(),
        TopologyError::DuplicateRoute
    );
    let existing = topology::compile(
        vec![provider()],
        vec![],
        vec![],
        vec![],
        vec![topology::CanonicalModel {
            id: route.canonical_model.clone(),
            task: topology::TaskKind::Generation,
            contract: topology::GenerationSemanticContract::full(),
        }],
    )
    .unwrap();
    assert_eq!(
        existing
            .with_images(vec![operation.clone()], vec![route.clone()])
            .unwrap_err(),
        TopologyError::TaskMismatch
    );
    let compiled = topology()
        .with_images(vec![operation], vec![route])
        .unwrap();
    assert!(compiled.model("public-image").is_none());
    let route = compiled.image_route("public-image").unwrap();
    let request = morphiecore::adapter::images::Request::new(
        "public-image",
        morphiecore::semantic::task::image_generation::ImageGenerationRequest::new("square")
            .unwrap(),
    );
    let prepared = prepare(
        route,
        &provider(),
        &SecretMaterial::new("synthetic-key").unwrap(),
        &request,
    )
    .unwrap();
    assert_eq!(prepared.origin, ORIGIN);
    assert_eq!(prepared.path, "/images/generations");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&prepared.body).unwrap(),
        serde_json::json!({"model":"private-image","prompt":"square"})
    );
    assert!(!format!("{prepared:?}").contains("synthetic-key"));
    let mut changed = request;
    changed.model = "another-label".into();
    assert!(
        prepare(
            route,
            &provider(),
            &SecretMaterial::new("synthetic-key").unwrap(),
            &changed
        )
        .is_err()
    );
}
#[test]
fn image_activation_is_explicit_and_requires_its_own_credential_binding() {
    let (operation, route) = super::image_support::binding(ORIGIN);
    let compiled = topology()
        .with_images(vec![operation], vec![route])
        .unwrap()
        .with_model_metadata([(
            ModelId::new("canonical-image").unwrap(),
            topology::ModelMetadata::new(1, "Synthetic Image Developer").unwrap(),
        )])
        .unwrap();
    let build = |model: &str, credentials| {
        Gateway::new_with_images(
            compiled.clone(),
            vec![],
            vec![ImageEntry {
                model: model.into(),
            }],
            credentials,
            SecretMaterial::new("synthetic-client-key-long-enough-0001").unwrap(),
            Limits::default(),
            None,
        )
    };
    assert!(build("public-image", Credentials::new()).is_err());
    let credentials = || {
        let mut c = Credentials::new();
        c.insert(
            CredentialBindingId::new("fixture-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-upstream-key").unwrap()),
        );
        c
    };
    assert!(build("unknown", credentials()).is_err());
    assert!(build("public-image", credentials()).is_ok());
}
