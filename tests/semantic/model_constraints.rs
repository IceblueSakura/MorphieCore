//! Model identity does not authorize a Provider to erase unsupported media.
use morphiecore::{
    adapter::{Adapter, Dialect},
    lowering::generation::GenerationRepresentationContract as Representation,
    protocol::openai::Profile,
    semantic::task::generation::{GenerationSemanticContract, ImageDetail, ImageFormat},
};
use serde_json::json;
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
