//! Field-selective client attribution projected across protocol targets.
//!
//! Attribution hints represent opaque end-user identifiers (e.g. `safety_identifier`
//! or legacy `user`).
//!
//! # Boundaries
//!
//! - **Non-Authoritative**: Attribution is strictly payload metadata on target copies.
//!   It is never used for authentication, caller identity, rate-limiting tenancy, or
//!   cache key computation.
//! - **Precedence**: When mapping between protocols, modern `safety_identifier` takes
//!   precedence over legacy `user` fields.
//! - **Explicit Null/Empty**: `null` and empty strings are treated as deliberate client
//!   choices and are never overridden by fallback.
//! - **Strict Validation**: Values violating target-specific constraints (such as Google
//!   Cloud label charset or 63-character limit) reject fail-closed without truncation
//!   or hashing.
use crate::{
    protocol::{ClientProtocol, CodecError, GenerationRequestEnvelope as Request, cache::HintRule},
    semantic::{context::ClientIdentityHints, value::Presence},
};

/// Supported identity carrier targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Carrier {
    /// OpenAI Chat and Responses carriers (`safety_identifier`, `user`).
    OpenAi,
    /// Anthropic Messages carrier (`metadata.user_id`, represented as `user`).
    Messages,
    /// Google Gemini Cloud labels (`labels.safety_identifier`).
    GoogleLabels,
    /// Target has no attribution carrier; all identity hints are omitted.
    None,
}

/// The result of projecting client identity hints onto a target carrier.
pub(crate) struct Projection {
    /// Projected identity fields suitable for target payload encoding.
    pub identity: ClientIdentityHints,
    /// Declared hint omission rules recording any lost fields.
    pub losses: Vec<HintRule>,
}

/// Projects request identity hints to the target carrier.
///
/// Follows exact-first mapping:
/// 1. Native transfers preserve matching fields without loss.
/// 2. Cross-protocol transfers prioritize `safety_identifier` over legacy `user`.
/// 3. Google Cloud label formatting is enforced strictly on projected values.
/// 4. Any remaining unrepresented hints produce [`HintRule::OmitIdentityHints`]
///    and [`HintRule::OmitLegacyUserBucketing`].
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

/// Validates Google Cloud Label values against standard Cloud resource constraints:
/// `[\p{Ll}\p{Lo}\p{N}_-]{0,63}` without normalization or truncation.
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
