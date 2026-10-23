//! Native lifecycle report validation precedes normalization and delivery.
use super::CodecError;
use crate::semantic::{
    context::{ResponseReports, ServiceClass},
    task::generation::{ReasoningContext, ReasoningMode, ReasoningSummary},
    value::Presence,
};

pub(super) fn relation(reports: &ResponseReports, resolution: bool) -> Result<(), CodecError> {
    reports.validate()?;
    let ResponseReports::Lifecycle {
        opening: Some(opening),
        terminal: Some(terminal),
    } = reports
    else {
        return Ok(());
    };
    if let (Some(before), Some(after)) = (&opening.settings, &terminal.settings) {
        let mut allowed = before.clone();
        if resolution {
            if before.reasoning.presentation.summary == Presence::Value(ReasoningSummary::Auto)
                && after.reasoning.presentation.summary
                    == Presence::Value(ReasoningSummary::Detailed)
            {
                allowed.reasoning.presentation.summary =
                    after.reasoning.presentation.summary.clone();
            }
            if before.reasoning.context.is_absent()
                && after.reasoning.context == Presence::Value(ReasoningContext::AllTurns)
            {
                allowed.reasoning.context = after.reasoning.context.clone();
            }
            if before.reasoning.mode.is_absent()
                && after.reasoning.mode == Presence::Value(ReasoningMode::Standard)
            {
                allowed.reasoning.mode = after.reasoning.mode.clone();
            }
        }
        if &allowed != after {
            return Err(CodecError::Invalid("reported settings changed"));
        }
    }
    let before = &opening.service_class;
    let after = &terminal.service_class;
    if !before.is_absent()
        && !after.is_absent()
        && before != after
        && !(resolution
            && *before == Presence::Value(ServiceClass::ConfiguredProcessing)
            && *after == Presence::Value(ServiceClass::StandardProcessing))
    {
        return Err(CodecError::Invalid("service tier changed"));
    }
    Ok(())
}

/// Admit one whole native snapshot. Missing fields never fill another phase.
pub(super) fn observe(
    previous: Option<&ResponseReports>,
    incoming: &ResponseReports,
    resolution: bool,
) -> Result<ResponseReports, CodecError> {
    let mut next = incoming.clone();
    if let Some(ResponseReports::Lifecycle { opening, terminal }) = previous {
        let ResponseReports::Lifecycle {
            opening: next_opening,
            terminal: next_terminal,
        } = &mut next
        else {
            return Err(CodecError::Invalid("report lifecycle changed"));
        };
        if next_opening.is_some() && terminal.is_some() {
            return Err(CodecError::Invalid("report phase regressed"));
        }
        for (old, new) in [(opening, next_opening), (terminal, next_terminal)] {
            if let (Some(old), Some(new)) = (old, new.as_ref())
                && old != new
            {
                return Err(CodecError::Invalid(if old.settings != new.settings {
                    "reported settings changed"
                } else {
                    "service tier changed"
                }));
            }
            if new.is_none() {
                *new = old.clone();
            }
        }
    }
    relation(&next, resolution)?;
    Ok(next)
}

/// Delivered contexts are cumulative; already published observations cannot
/// disappear or change when a later phase is supplied.
pub(super) fn update(previous: &ResponseReports, next: &ResponseReports) -> Result<(), CodecError> {
    relation(next, true)?;
    match (previous, next) {
        (
            ResponseReports::Lifecycle { opening, terminal },
            ResponseReports::Lifecycle {
                opening: new_opening,
                terminal: new_terminal,
            },
        ) => {
            if opening.is_some() && opening != new_opening
                || terminal.is_some() && terminal != new_terminal
            {
                return Err(CodecError::Invalid("reported observation changed"));
            }
        }
        (ResponseReports::Single(old), ResponseReports::Single(new)) => {
            if !old.service_class.is_absent() && old.service_class != new.service_class {
                return Err(CodecError::Invalid("reported service changed"));
            }
        }
        _ => return Err(CodecError::Invalid("report lifecycle changed")),
    }
    Ok(())
}

/// A snapshot is not a lifecycle archive. Return whether an opening observation
/// was omitted so static delivery can record its own loss, unlike full SSE.
pub(crate) fn terminal_view(reports: &mut ResponseReports) -> Result<bool, CodecError> {
    relation(reports, true)?;
    if let ResponseReports::Lifecycle { opening, terminal } = reports {
        if terminal.is_none() {
            return Err(CodecError::Invalid("terminal report missing"));
        }
        return Ok(opening.take().is_some());
    }
    Ok(false)
}

pub(super) fn opening_view(reports: &mut ResponseReports) -> Result<(), CodecError> {
    relation(reports, true)?;
    if let ResponseReports::Lifecycle { terminal, .. } = reports {
        *terminal = None;
    }
    Ok(())
}

/// Existing single-report protocols cannot express changing lifecycle classes.
pub(crate) fn validate_single(reports: &ResponseReports) -> Result<(), CodecError> {
    relation(reports, true)?;
    if let ResponseReports::Lifecycle {
        opening: Some(opening),
        terminal: Some(terminal),
    } = reports
        && opening.service_class != terminal.service_class
    {
        return Err(CodecError::Unsupported("lifecycle service reports".into()));
    }
    Ok(())
}
pub(crate) fn single_view(reports: &mut ResponseReports) -> Result<(), CodecError> {
    validate_single(reports)?;
    if let ResponseReports::Lifecycle { opening, terminal } = reports {
        let report = terminal
            .as_deref()
            .or(opening.as_deref())
            .cloned()
            .unwrap_or_default();
        *reports = ResponseReports::single(report);
    }
    Ok(())
}
