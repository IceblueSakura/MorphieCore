//! Native reported processing classes, separate from request policy and accounting.
use crate::{
    protocol::{
        CodecError,
        failure::{CarrierReason, ShapeReason},
    },
    semantic::{context::ServiceClass, value::Presence},
};
use serde_json::{Map, Value};

pub(super) fn read(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Presence<ServiceClass>, CodecError> {
    Ok(match object.get(field) {
        None => Presence::Absent,
        Some(Value::String(value)) => Presence::Value(match value.as_str() {
            "standard" => ServiceClass::StandardProcessing,
            "flex" => ServiceClass::FlexibleProcessing,
            "priority" => ServiceClass::PriorityProcessing,
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Field(
                    crate::protocol::conversion::generation::Field::ServiceClass,
                )));
            }
        }),
        Some(_) => {
            return Err(CodecError::Invalid(ShapeReason::Metadata));
        }
    })
}
