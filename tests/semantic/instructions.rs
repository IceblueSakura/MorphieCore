//! Instruction authority, controls and deletion across independent protocol projections.
use morphiecore::{
    lowering::generation::{GenerationRepresentationContract as Contract, check, lower_request},
    protocol::openai::{DecodedRequest, Profile, chat, responses},
    semantic::task::generation::{GenerationRequirements, Item},
};
use serde_json::{Value, json};

fn encode(d: &DecodedRequest, profile: Profile) -> Value {
    let target = lower_request(&d.semantic, &d.fidelity, profile, Contract::full()).unwrap();
    match profile {
        Profile::Chat => chat::encode_generation(&target).unwrap(),
        Profile::Responses => responses::encode_generation(&target).unwrap(),
    }
}

#[test]
fn instruction_authority_and_controls_have_independent_wire_expectations() {
    let chat_wire = json!({"messages":[{"role":"developer","content":"Be brief."},{"role":"user","content":"Hello"}],"temperature":0.25,"max_completion_tokens":64});
    let responses_wire = json!({"input":[{"role":"developer","content":"Be brief."},{"type":"message","role":"user","content":[{"type":"input_text","text":"Hello"}]}],"temperature":0.25,"max_output_tokens":64});
    for d in [
        chat::decode_generation(&chat_wire).unwrap(),
        responses::decode_generation(&responses_wire).unwrap(),
    ] {
        let requirements = GenerationRequirements::derive(&d.semantic);
        assert_eq!(requirements.instruction_count, 1);
        assert!(requirements.temperature);
        assert_eq!(requirements.max_output_tokens, Some(64));
        assert_eq!(encode(&d, Profile::Chat), chat_wire);
        assert_eq!(encode(&d, Profile::Responses), responses_wire);
    }
}

#[test]
fn ir_deletion_controls_both_encoders() {
    let mut d = chat::decode_generation(&json!({"messages":[{"role":"system","content":"obsolete"},{"role":"user","content":"keep"}]})).unwrap();
    d.semantic = d
        .semantic
        .retain_items(|_, item| !matches!(item, Item::Instruction(_)))
        .unwrap();
    assert_eq!(
        encode(&d, Profile::Chat),
        json!({"messages":[{"role":"user","content":"keep"}]})
    );
    assert_eq!(
        encode(&d, Profile::Responses),
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"keep"}]}]})
    );
}

#[test]
fn unrepresentable_instructions_fail_before_encoding() {
    let d = chat::decode_generation(&json!({"messages":[{"role":"system","content":"required"},{"role":"user","content":"hello"}]})).unwrap();
    let contract = Contract {
        semantics: morphiecore::semantic::task::generation::GenerationSemanticContract {
            instructions: false,
            ..morphiecore::semantic::task::generation::GenerationSemanticContract::full()
        },
        ..Contract::full()
    };
    assert!(check(&d.semantic, contract).is_err());
}
