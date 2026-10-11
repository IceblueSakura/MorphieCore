use super::speech_support::*;
use morphiecore::{
    provider::SecretMaterial,
    topology::{self},
};
const ORIGIN: &str = "https://synthetic.invalid";

#[test]
fn speech_activation_requires_metadata_single_source_credentials_and_explicit_entry() {
    use morphiecore::{
        gateway::{Credentials, Gateway, Limits, SpeechEntry},
        provider::CredentialBindingId,
    };
    use std::sync::Arc;
    let credentials = || {
        let mut c = Credentials::new();
        c.insert(
            CredentialBindingId::new("speech-key").unwrap(),
            Arc::new(SecretMaterial::new("synthetic-key").unwrap()),
        );
        c
    };
    let key = || SecretMaterial::new("synthetic-client-key-long-enough").unwrap();
    let entry = || SpeechEntry {
        model: "public-speech".into(),
    };
    assert!(
        Gateway::new(
            topology(ORIGIN),
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
    assert!(
        Gateway::new_with_media(
            topology(ORIGIN),
            vec![],
            vec![],
            vec![entry()],
            vec![],
            Credentials::new(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
    assert!(
        Gateway::new_with_media(
            topology(ORIGIN),
            vec![],
            vec![],
            vec![entry(), entry()],
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
    assert!(
        Gateway::new_with_media(
            topology(ORIGIN),
            vec![],
            vec![],
            vec![entry()],
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_ok()
    );
    for speech_bytes in [0, morphiecore::semantic::value::MAX_AUDIO_BYTES + 1] {
        assert!(
            Gateway::new_with_media(
                topology(ORIGIN),
                vec![],
                vec![],
                vec![entry()],
                vec![],
                credentials(),
                key(),
                Limits {
                    speech_bytes,
                    ..Limits::default()
                },
                None
            )
            .is_err()
        );
    }
    let (operation, route) = binding(ORIGIN);
    let no_metadata = topology::compile(vec![provider(ORIGIN)], vec![], vec![], vec![], vec![])
        .unwrap()
        .with_speech(vec![operation], vec![route])
        .unwrap();
    assert!(
        Gateway::new_with_media(
            no_metadata,
            vec![],
            vec![],
            vec![entry()],
            vec![],
            credentials(),
            key(),
            Limits::default(),
            None
        )
        .is_err()
    );
}
