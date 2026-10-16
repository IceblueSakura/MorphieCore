//! Native service tokens belong here, not in semantic enum serialization.
//! Chat excludes `ultrafast`; request and reported vocabularies remain independent.
//! https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/chat/completion_create_params.py
use super::{
    CodecError, Profile,
    common::{put_presence, read_presence},
};
use crate::semantic::{
    context::{ServiceClass, ServicePolicy},
    value::Presence,
};
use serde_json::{Map, Value, json};

pub(super) fn read_policy(
    o: &Map<String, Value>,
    profile: Profile,
) -> Result<Presence<ServicePolicy>, CodecError> {
    read_presence(o, "service_tier", |v| {
        Ok(match v.as_str() {
            Some("auto") => ServicePolicy::ProjectConfigured,
            Some("default") => ServicePolicy::StandardProcessing,
            Some("flex") => ServicePolicy::FlexibleProcessing,
            Some("fast") => ServicePolicy::FastProcessing,
            Some("priority") => ServicePolicy::PriorityProcessing,
            Some("scale") => ServicePolicy::ScaleProcessing,
            Some("ultrafast") if profile == Profile::Responses => {
                ServicePolicy::UltraFastProcessing
            }
            _ => return Err(CodecError::Invalid("service policy")),
        })
    })
}
pub(super) fn policy_wire(
    policy: ServicePolicy,
    profile: Profile,
) -> Result<&'static str, CodecError> {
    Ok(match policy {
        ServicePolicy::ProjectConfigured => "auto",
        ServicePolicy::StandardProcessing => "default",
        ServicePolicy::FlexibleProcessing => "flex",
        ServicePolicy::FastProcessing => "fast",
        ServicePolicy::PriorityProcessing => "priority",
        ServicePolicy::ScaleProcessing => "scale",
        ServicePolicy::UltraFastProcessing if profile == Profile::Responses => "ultrafast",
        _ => return Err(CodecError::Unsupported("service policy".into())),
    })
}
pub(super) fn write_policy(
    o: &mut Map<String, Value>,
    policy: &Presence<ServicePolicy>,
    profile: Profile,
) -> Result<(), CodecError> {
    if let Some(value) = policy.value() {
        policy_wire(*value, profile)?;
    }
    put_presence(o, "service_tier", policy, |value| {
        json!(policy_wire(*value, profile).expect("validated service policy"))
    });
    Ok(())
}
pub(super) fn read_class(
    o: &Map<String, Value>,
    profile: Profile,
) -> Result<Presence<ServiceClass>, CodecError> {
    read_presence(o, "service_tier", |v| {
        Ok(match v.as_str() {
            Some("auto") => ServiceClass::ConfiguredProcessing,
            Some("default") => ServiceClass::StandardProcessing,
            Some("flex") => ServiceClass::FlexibleProcessing,
            Some("fast") => ServiceClass::FastProcessing,
            Some("priority") => ServiceClass::PriorityProcessing,
            Some("scale") => ServiceClass::ScaleProcessing,
            Some("ultrafast") if profile == Profile::Responses => ServiceClass::UltraFastProcessing,
            _ => return Err(CodecError::Invalid("reported service")),
        })
    })
}
pub(super) fn class_wire(
    class: ServiceClass,
    profile: Profile,
) -> Result<&'static str, CodecError> {
    Ok(match class {
        ServiceClass::ConfiguredProcessing => "auto",
        ServiceClass::StandardProcessing => "default",
        ServiceClass::FlexibleProcessing => "flex",
        ServiceClass::FastProcessing => "fast",
        ServiceClass::PriorityProcessing => "priority",
        ServiceClass::ScaleProcessing => "scale",
        ServiceClass::UltraFastProcessing if profile == Profile::Responses => "ultrafast",
        _ => return Err(CodecError::Unsupported("reported service".into())),
    })
}
pub(super) fn write_class(
    o: &mut Map<String, Value>,
    class: &Presence<ServiceClass>,
    profile: Profile,
) -> Result<(), CodecError> {
    if let Some(value) = class.value() {
        class_wire(*value, profile)?;
    }
    put_presence(o, "service_tier", class, |value| {
        json!(class_wire(*value, profile).expect("validated reported service"))
    });
    Ok(())
}
