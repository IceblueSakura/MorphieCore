//! Built-in authorization drivers and their shared interface.
pub(in crate::credential) mod driver;
pub(in crate::credential) mod grok;
pub mod siwc;
use crate::credential::{
    AuthDriver, CredentialError,
    drivers::{grok::GrokAuthority, siwc::SiwcAuthority},
};
use std::sync::Arc;
pub fn builtin_drivers(proxy: Option<&str>) -> Result<Vec<Arc<dyn AuthDriver>>, CredentialError> {
    Ok(vec![
        Arc::new(GrokAuthority::new(proxy)?),
        Arc::new(SiwcAuthority::new(proxy)?),
    ])
}
