//! Messages capacity policy and reported service have different native vocabularies.
use super::shape::{nonnull, presence, put};
use crate::{
    protocol::{CodecError, failure::CarrierReason},
    semantic::{
        context::{ServiceClass, ServicePolicy},
        value::Presence,
    },
};
use serde_json::{Map, Value, json};

pub(super) fn read_policy(o: &Map<String, Value>) -> Result<Option<ServicePolicy>, CodecError> {
    let policy = presence(o, "service_tier", |v| {
        Ok(match v.as_str() {
            Some("auto") => ServicePolicy::PreferCommittedCapacity,
            Some("standard_only") => ServicePolicy::StandardCapacityOnly,
            _ => {
                return Err(CodecError::invalid());
            }
        })
    })?;
    nonnull(&policy)?;
    Ok(policy.value().copied())
}
pub(crate) fn policy_wire(policy: ServicePolicy) -> Result<&'static str, CodecError> {
    match policy {
        ServicePolicy::PreferCommittedCapacity => Ok("auto"),
        ServicePolicy::StandardCapacityOnly => Ok("standard_only"),
        _ => Err(CodecError::Unsupported(CarrierReason::Field(
            crate::protocol::conversion::generation::Field::ServiceClass,
        ))),
    }
}
pub(super) fn read_class(o: &Map<String, Value>) -> Result<Presence<ServiceClass>, CodecError> {
    presence(o, "service_tier", |v| {
        Ok(match v.as_str() {
            Some("standard") => ServiceClass::StandardCapacity,
            Some("priority") => ServiceClass::PriorityCapacity,
            Some("batch") => ServiceClass::BatchProcessing,
            _ => {
                return Err(CodecError::invalid());
            }
        })
    })
}
pub(super) fn class_wire(class: ServiceClass) -> Result<&'static str, CodecError> {
    match class {
        ServiceClass::StandardCapacity => Ok("standard"),
        ServiceClass::PriorityCapacity => Ok("priority"),
        ServiceClass::BatchProcessing => Ok("batch"),
        _ => Err(CodecError::Unsupported(CarrierReason::Field(
            crate::protocol::conversion::generation::Field::ServiceClass,
        ))),
    }
}
pub(super) fn write_class(
    o: &mut Map<String, Value>,
    class: &Presence<ServiceClass>,
) -> Result<(), CodecError> {
    if let Some(value) = class.value() {
        class_wire(*value)?;
    }
    put(o, "service_tier", class, |value| {
        json!(class_wire(*value).expect("validated reported service"))
    });
    Ok(())
}
