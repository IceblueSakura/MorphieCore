//! Native reported processing classes, separate from request policy and accounting.
use crate::{
    protocol::{
        CodecError,
        failure::{CarrierReason, ShapeReason},
    },
    semantic::context::ServiceClass,
};
use serde_json::{Map, Value};

pub(super) fn read(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<ServiceClass>, CodecError> {
    Ok(match object.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(match value.as_str() {
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
