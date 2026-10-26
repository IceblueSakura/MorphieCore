//! Text responses, admission, events, terminals and open upstream decoding.
#[path = "text/lifecycle_reports.rs"]
mod lifecycle_reports;
#[path = "text/open_decoding.rs"]
mod open_decoding;
#[path = "text/response.rs"]
mod response;
#[path = "text/text_admission.rs"]
mod text_admission;
#[path = "text/text_events.rs"]
mod text_events;
#[path = "text/text_profile.rs"]
mod text_profile;
#[path = "text/timestamps.rs"]
mod timestamps;
#[test]
fn responses_original_error_classification_is_not_the_sse_discriminator() {
    use morphiecore::protocol::openai::{Dialect, GenerationApi, GenerationCodec, TargetContract};
    use serde_json::json;
    let adapter = GenerationCodec::new(GenerationApi::Responses, Dialect::OpenRouter, None);
    let wire = json!({"id":"r","object":"response","model":"m","created_at":1,"status":"failed","output":[],
        "error":{"code":"server_error","message":"failed","type":"upstream_classification"}});
    let parsed = adapter
        .decode_response(wire.to_string().as_bytes())
        .unwrap();
    assert_eq!(
        parsed
            .semantic
            .details()
            .error
            .as_ref()
            .unwrap()
            .original_type
            .as_ref()
            .unwrap()
            .as_str(),
        "upstream_classification"
    );
    let mut response = parsed.clone();
    response.semantic = response
        .semantic
        .with_details(morphiecore::semantic::task::generation::TerminalDetails {
            error: Some(morphiecore::semantic::task::generation::ResponseError {
                code: Some(crate::events_support::text("server_error")),
                message: crate::events_support::text("failed"),
                param: None,
                original_type: Some(crate::events_support::text("independent_type")),
            }),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        adapter
            .encode_response(&response, &TargetContract::full())
            .unwrap()["error"],
        json!({"code":"server_error","message":"failed","type":"independent_type"})
    );
    let mut malformed = wire;
    malformed["error"]["type"] = json!(7);
    assert!(
        adapter
            .decode_response(malformed.to_string().as_bytes())
            .is_err()
    );
}
