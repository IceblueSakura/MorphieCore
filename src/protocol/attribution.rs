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
//! - **Target Constraints**: Unrepresentable attribution is omitted with a named
//!   loss, never truncated, hashed or replaced by a fallback identifier.
use crate::{
    lowering::disposition::{OMIT_IDENTITY_HINTS, OMIT_USER_BUCKETING, Rule},
    protocol::{ClientProtocol, CodecError, GenerationRequestEnvelope as Request},
    semantic::context::ClientIdentityHints,
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
    pub losses: Vec<Rule>,
}

/// Projects request identity hints to the target carrier.
///
/// Follows exact-first mapping:
/// 1. Native transfers preserve matching fields without loss.
/// 2. Cross-protocol transfers prioritize `safety_identifier` over legacy `user`.
/// 3. Google Cloud label formatting is enforced strictly on projected values.
/// 4. Remaining unrepresented hints use the shared identity/bucketing dispositions.
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
            remaining.user = None;
        }
        Carrier::None => {}
        _ => {
            // An explicit empty identifier still prevents fallback.
            let selected = if !remaining.safety_identifier.is_none() {
                &mut remaining.safety_identifier
            } else {
                &mut remaining.user
            };
            {
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
        Carrier::GoogleLabels
            if identity
                .safety_identifier
                .as_ref()
                .is_some_and(|value| validate_label_value(value).is_err()) =>
        {
            remaining.safety_identifier = identity.safety_identifier.take();
        }
        _ => {}
    }
    let mut losses = vec![];
    if remaining != ClientIdentityHints::default() {
        losses.push(OMIT_IDENTITY_HINTS);
    }
    if openai_source && source.user.is_some() && !matches!(carrier, Carrier::OpenAi) {
        losses.push(OMIT_USER_BUCKETING);
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
