//! Model identity does not authorize a Provider to erase unsupported media.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Representation,
    protocol::openai::Profile,
    semantic::task::generation::{GenerationSemanticContract, ImageDetail, ImageFormat},
};
use serde_json::json;
#[test]
fn native_audio_binding_is_explicit_chat_input_with_text_only_output() {
    use morphiecore::topology::{ProtocolProfile, catalog::API_KEY_BINDINGS};
    let binding = API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "gpt-audio-mini")
        .unwrap();
    assert_eq!(binding.upstream, "openai/gpt-audio-mini");
    assert_eq!(binding.protocols, &[ProtocolProfile::OpenAiChat]);
    let endpoint = binding.endpoint(ProtocolProfile::OpenAiChat).unwrap();
    assert!(endpoint.representation.semantics.audio_input);
    assert!(!endpoint.representation.semantics.audio_output);
    assert!(!endpoint.representation.semantics.audio_history);
    assert!(!endpoint.representation.semantics.reasoning);
    let client = Adapter::new(Profile::Chat, Dialect::Standard, None);
    let body = json!({"model":"gpt-audio-mini","modalities":["text"],"messages":[
        {"role":"user","content":[{"type":"input_audio","input_audio":{"format":"wav","data":"AQID"}}]}]});
    let input = client
        .decode_request(&body.to_string().into_bytes())
        .unwrap();
    assert!(
        input
            .check_semantic(&binding.public_model().contract)
            .is_ok()
    );
    let upstream = Adapter::new(Profile::Chat, binding.dialect, None);
    let sent = upstream
        .encode_request(&input, binding.upstream, &endpoint.representation)
        .unwrap();
    assert_eq!(sent["messages"], body["messages"]);
    assert_eq!(sent["modalities"], json!(["text"]));
    let unrelated = API_KEY_BINDINGS
        .iter()
        .find(|b| b.model == "gpt-6-luna")
        .unwrap();
    assert!(
        input
            .check_semantic(&unrelated.public_model().contract)
            .is_err()
    );
}

#[test]
fn same_model_keeps_semantics_while_each_target_projects_media_independently() {
    let contract = GenerationSemanticContract::text_images();
    let client = Adapter::new(Profile::Responses, Dialect::MorphieCore, None);
    let request=client.decode_request(&serde_json::to_vec(&json!({"model":"synthetic-model","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/bmp;base64,AQ=="}]}]})).unwrap()).unwrap();
    assert!(request.check_semantic(&contract).is_ok());
    let original = request.clone();
    let mut limited = Representation::full();
    limited.images.inline_formats = vec![ImageFormat::Png];
    assert!(
        client
            .encode_request(&request, "provider-a-alias", &limited)
            .is_err()
    );
    let mut compatible = Representation::full();
    compatible.images.inline_formats = vec![ImageFormat::Bmp];
    let wire = client
        .encode_request(&request, "provider-b-alias", &compatible)
        .unwrap();
    assert_eq!(wire["model"], "provider-b-alias");
    assert_eq!(
        wire["input"][0]["content"][0]["image_url"],
        "data:image/bmp;base64,AQ=="
    );
    assert_eq!(request, original);
    compatible.images.max_images = 0;
    assert!(client.encode_request(&request, "b", &compatible).is_err());
    compatible.images.max_images = 1;
    compatible.images.max_inline_bytes = 0;
    assert!(client.encode_request(&request, "b", &compatible).is_err());
    compatible.images.max_inline_bytes = 1;
    assert!(client.encode_request(&request, "b", &compatible).is_ok());
    let mut value = json!({"model":"synthetic-model","input":[{"role":"user","content":[{"type":"input_image","image_url":"https://example.invalid/x","detail":"auto"}]}]});
    let detail = client
        .decode_request(&serde_json::to_vec(&value).unwrap())
        .unwrap();
    compatible.images.details = vec![ImageDetail::High];
    assert!(client.encode_request(&detail, "b", &compatible).is_err());
    value["input"][0]["content"][0]
        .as_object_mut()
        .unwrap()
        .remove("detail");
    let omitted = client
        .decode_request(&serde_json::to_vec(&value).unwrap())
        .unwrap();
    assert!(client.encode_request(&omitted, "b", &compatible).is_ok());
}

#[test]
fn grok_image_input_and_opaque_include_reach_the_fixed_responses_endpoint() {
    use morphiecore::topology::{ProtocolProfile, catalog::SUBSCRIPTION_BINDINGS};
    let binding = SUBSCRIPTION_BINDINGS
        .iter()
        .find(|binding| binding.profile == "grok")
        .unwrap();
    let endpoint = binding.endpoint();
    assert_eq!(endpoint.protocol, ProtocolProfile::OpenAiResponses);
    assert_eq!(endpoint.target.origin.as_str(), "https://api.x.ai");
    assert_eq!(endpoint.target.path.as_str(), "/v1/responses");
    let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let body = json!({"model":binding.model,"store":false,
        "include":["reasoning.encrypted_content"],
        "input":[{"role":"user","content":[
            {"type":"input_text","text":"before"},
            {"type":"input_image","image_url":"data:image/png;base64,AQID"},
            {"type":"input_text","text":"after"}]}]});
    let request = client.decode_request(body.to_string().as_bytes()).unwrap();
    request
        .check_semantic(&binding.public_model().contract)
        .unwrap();
    assert!(endpoint.representation.semantics.image_input);
    assert!(!endpoint.representation.semantics.file_input);
    assert!(!endpoint.representation.semantics.tool_result_images);
    let upstream = Adapter::new(Profile::Responses, binding.dialect, None)
        .encode_request(&request, binding.upstream, &endpoint.representation)
        .unwrap();
    assert_eq!(
        upstream["input"],
        json!([{"type":"message","role":"user","content":body["input"][0]["content"]}])
    );
    assert_eq!(upstream["include"], body["include"]);
    assert_eq!(upstream["store"], false);
}

#[test]
fn selected_production_contracts_do_not_infer_tool_images_from_user_images() {
    let topology = morphiecore::topology::catalog::default_topology().unwrap();
    let client = Adapter::new(Profile::Responses, Dialect::Standard, None);
    let tool_image = client
        .decode_request(
            json!({"model":"synthetic","input":[
        {"type":"function_call","name":"lookup","call_id":"a","arguments":"{}"},
        {"type":"function_call_output","call_id":"a","output":[
            {"type":"input_text","text":"before"},
            {"type":"input_image","image_url":"data:image/png;base64,AQID"},
            {"type":"input_text","text":"after"}]}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    for model in [
        "mimo-v2.6-flash",
        "mimo-v2.6-pro",
        "deepseek-flash",
        "gpt-6-luna",
        "gpt-6.1-sol",
        "grok-4.7",
    ] {
        let public = topology.model(model).unwrap();
        assert!(!public.contract.tool_result_images);
        assert!(tool_image.check_semantic(&public.contract).is_err());
        for endpoint in topology.route_endpoints(&public.route) {
            assert!(!endpoint.representation.semantics.tool_result_images);
            assert!(
                tool_image
                    .check_semantic(&endpoint.representation.semantics)
                    .is_err()
            );
        }
    }
    let image = client
        .decode_request(
            json!({"model":"synthetic","input":[
        {"role":"user","content":[
            {"type":"input_image","image_url":"data:image/png;base64,AQID"}]}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(
        image
            .check_semantic(&topology.model("gpt-6.1-sol").unwrap().contract)
            .is_err()
    );
}
