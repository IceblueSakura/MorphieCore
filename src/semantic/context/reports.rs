//! Bounded reported observations, not requested settings or an event archive.
use super::{ContextError, ServiceClass};
use crate::semantic::{
    task::generation::{GenerationSettings, MAX_TOTAL_BYTES},
    value::Presence,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResponseReport {
    pub settings: Option<GenerationSettings>,
    pub service_class: Presence<ServiceClass>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReportPhase {
    Opening,
    Terminal,
}

/// Single reports have no lifecycle distinction. Lifecycle slots preserve whole
/// observations; neither a missing slot nor a missing field inherits another.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponseReports {
    Single(Box<ResponseReport>),
    Lifecycle {
        opening: Option<Box<ResponseReport>>,
        terminal: Option<Box<ResponseReport>>,
    },
}
impl Default for ResponseReports {
    fn default() -> Self {
        Self::Single(Box::default())
    }
}
impl ResponseReports {
    pub fn single(report: ResponseReport) -> Self {
        Self::Single(Box::new(report))
    }
    pub fn phased(phase: ReportPhase, report: ResponseReport) -> Self {
        let report = Some(Box::new(report));
        match phase {
            ReportPhase::Opening => Self::Lifecycle {
                opening: report,
                terminal: None,
            },
            ReportPhase::Terminal => Self::Lifecycle {
                opening: None,
                terminal: report,
            },
        }
    }
    pub fn at(&self, phase: ReportPhase) -> Option<&ResponseReport> {
        match self {
            Self::Single(report) => Some(report),
            Self::Lifecycle { opening, terminal } => match phase {
                ReportPhase::Opening => opening.as_deref(),
                ReportPhase::Terminal => terminal.as_deref(),
            },
        }
    }
    /// The latest observation is a view, never an independently mutable copy.
    pub fn latest(&self) -> Option<&ResponseReport> {
        match self {
            Self::Single(report) => Some(report),
            Self::Lifecycle { opening, terminal } => terminal.as_deref().or(opening.as_deref()),
        }
    }
    pub fn latest_mut(&mut self) -> Option<&mut ResponseReport> {
        match self {
            Self::Single(report) => Some(report),
            Self::Lifecycle { opening, terminal } => {
                terminal.as_deref_mut().or(opening.as_deref_mut())
            }
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &ResponseReport> {
        let pair = match self {
            Self::Single(report) => [Some(report.as_ref()), None],
            Self::Lifecycle { opening, terminal } => [opening.as_deref(), terminal.as_deref()],
        };
        pair.into_iter().flatten()
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ResponseReport> {
        let pair = match self {
            Self::Single(report) => [Some(report.as_mut()), None],
            Self::Lifecycle { opening, terminal } => {
                [opening.as_deref_mut(), terminal.as_deref_mut()]
            }
        };
        pair.into_iter().flatten()
    }
    pub fn validate(&self) -> Result<usize, ContextError> {
        let mut bytes = 0usize;
        for report in self.iter() {
            bytes = bytes
                .checked_add(report.settings.as_ref().map_or(Ok(0), |s| s.validate())?)
                .and_then(|n| n.checked_add(std::mem::size_of::<ResponseReport>()))
                .ok_or(ContextError::Limit)?;
            if bytes > MAX_TOTAL_BYTES {
                return Err(ContextError::Limit);
            }
        }
        Ok(bytes)
    }
}
