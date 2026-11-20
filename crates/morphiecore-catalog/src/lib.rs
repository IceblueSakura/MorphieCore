//! Trusted Provider catalog and compiled model/binding topology for MorphieCore.

pub mod provider;
pub mod topology;

pub(crate) use morphiecore_protocol as protocol;
pub(crate) use morphiecore_protocol::adapter;
pub(crate) use morphiecore_protocol::lowering;
pub(crate) use morphiecore_semantic as semantic;
