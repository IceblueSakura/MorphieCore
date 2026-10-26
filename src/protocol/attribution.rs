//! Field-selective attribution on target copies; never authentication or cache identity.
use crate::{
    adapter::{ClientProtocol, Request},
    protocol::{CodecError, cache::HintRule},
    semantic::{context::ClientIdentityHints, value::Presence},
};

#[derive(Clone, Copy)]
pub(crate) enum Carrier {
    OpenAi,
    Messages,
    GoogleLabels,
    None,
}
pub(crate) struct Projection {
    pub identity: ClientIdentityHints,
    pub losses: Vec<HintRule>,
}

pub(crate) fn project(request: &Request, carrier: Carrier) -> Result<Projection, CodecError> {
    let source = &request.context.identity;
    source.validate()?;
    let openai_source = matches!(
        request.source,
        Some(ClientProtocol::Chat | ClientProtocol::Responses)
    );
    if openai_source {
        crate::protocol::openai::validate_identity(source)?;
    }
    let mut remaining = source.clone();
    let mut identity = ClientIdentityHints::default();
    let native_openai = matches!(
        request.source,
        None | Some(ClientProtocol::Chat | ClientProtocol::Responses)
    );
    let native_response = matches!(request.source, None | Some(ClientProtocol::Messages));
    match carrier {
        Carrier::OpenAi if native_openai => {
            identity = source.clone();
            remaining = Default::default();
        }
        Carrier::Messages if native_response => {
            identity.user = source.user.clone();
            remaining.user = Presence::Absent;
        }
        Carrier::None => {}
        _ => {
            // Null and empty are explicit choices, not opportunities to fall back.
            let selected = if !remaining.safety_identifier.is_absent() {
                &mut remaining.safety_identifier
            } else {
                &mut remaining.user
            };
            if !matches!(carrier, Carrier::GoogleLabels) || *selected != Presence::Null {
                let value = std::mem::take(selected);
                match carrier {
                    Carrier::Messages => identity.user = value,
                    Carrier::OpenAi | Carrier::GoogleLabels => identity.safety_identifier = value,
                    Carrier::None => unreachable!(),
                }
            }
        }
    }
    match carrier {
        Carrier::OpenAi => crate::protocol::openai::validate_identity(&identity)?,
        Carrier::GoogleLabels => {
            if let Some(value) = identity.safety_identifier.value() {
                validate_label_value(value)?;
            }
        }
        _ => {}
    }
    let mut losses = vec![];
    if remaining != ClientIdentityHints::default() {
        losses.push(HintRule::OmitIdentityHints);
    }
    if openai_source && !source.user.is_absent() && !matches!(carrier, Carrier::OpenAi) {
        losses.push(HintRule::OmitLegacyUserBucketing);
    }
    Ok(Projection { identity, losses })
}

fn validate_label_value(value: &str) -> Result<(), CodecError> {
    use unicode_general_category::{GeneralCategory as Category, get_general_category};
    // Unified Cloud labels: [\p{Ll}\p{Lo}\p{N}_-]{0,63}. No normalization.
    // https://generativelanguage.googleapis.com/$discovery/rest?version=v1beta
    if value.chars().count() > 63
        || value.chars().any(|c| {
            !matches!(c, '_' | '-')
                && !matches!(
                    get_general_category(c),
                    Category::LowercaseLetter
                        | Category::OtherLetter
                        | Category::DecimalNumber
                        | Category::LetterNumber
                        | Category::OtherNumber
                )
        })
    {
        return Err(CodecError::invalid());
    }
    Ok(())
}
