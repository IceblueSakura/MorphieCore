//! Local SDK exporters: enqueue owned records; serialization and I/O stay on the writer.
use super::writer::{Message, Output};
use opentelemetry::{InstrumentationScope, Value as AttributeValue, logs::AnyValue};
use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    logs::{LogBatch, LogExporter, SdkLogRecord},
    trace::{SpanData, SpanExporter},
};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub(super) struct LocalExporter {
    output: Output,
    resource: Arc<Resource>,
}
impl LocalExporter {
    pub(super) fn new(output: Output) -> Self {
        Self {
            output,
            resource: Arc::new(Resource::builder_empty().build()),
        }
    }
}
impl std::fmt::Debug for LocalExporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalExporter")
    }
}
impl LogExporter for LocalExporter {
    async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
        let mut failed = false;
        for (record, scope) in batch.iter() {
            failed |= !self.output.send(Message::Log(Box::new(LocalLog {
                record: record.clone(),
                scope: scope.clone(),
                resource: self.resource.clone(),
            })));
        }
        if failed {
            Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                "local telemetry queue unavailable".into(),
            ))
        } else {
            Ok(())
        }
    }
    fn set_resource(&mut self, resource: &Resource) {
        self.resource = Arc::new(resource.clone());
    }
}
impl SpanExporter for LocalExporter {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        let mut failed = false;
        for span in batch {
            failed |= !self.output.send(Message::Span(Box::new(LocalSpan {
                span,
                resource: self.resource.clone(),
            })));
        }
        if failed {
            Err(opentelemetry_sdk::error::OTelSdkError::InternalFailure(
                "local telemetry queue unavailable".into(),
            ))
        } else {
            Ok(())
        }
    }
    fn set_resource(&mut self, resource: &Resource) {
        self.resource = Arc::new(resource.clone());
    }
}

pub(super) struct LocalLog {
    record: SdkLogRecord,
    scope: InstrumentationScope,
    resource: Arc<Resource>,
}
pub(super) struct LocalSpan {
    span: SpanData,
    resource: Arc<Resource>,
}

// OTLP AnyValue has no null/u64 variant. Omit unknown optional map members;
// retain wide integers as exact decimal strings, never rounded doubles. A null
// array element is unsupported rather than silently changing array positions.
pub(super) fn body(value: Value) -> Result<AnyValue, ()> {
    Ok(match value {
        Value::Null => return Err(()),
        Value::Bool(v) => AnyValue::Boolean(v),
        Value::String(v) => AnyValue::String(v.into()),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                AnyValue::Int(i)
            } else if v.as_u64().is_some() {
                AnyValue::String(v.to_string().into())
            } else {
                AnyValue::Double(v.as_f64().filter(|v| v.is_finite()).ok_or(())?)
            }
        }
        Value::Array(v) => {
            AnyValue::ListAny(Box::new(v.into_iter().map(body).collect::<Result<_, _>>()?))
        }
        Value::Object(v) => AnyValue::Map(Box::new(
            v.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| Ok((k.into(), body(v)?)))
                .collect::<Result<_, ()>>()?,
        )),
    })
}
fn json_body(value: &AnyValue) -> Result<Value, ()> {
    Ok(match value {
        AnyValue::Int(v) => json!(v),
        AnyValue::Double(v) => json!(v),
        AnyValue::Boolean(v) => json!(v),
        AnyValue::String(v) => json!(v.as_str()),
        AnyValue::ListAny(v) => Value::Array(v.iter().map(json_body).collect::<Result<_, _>>()?),
        AnyValue::Map(v) => Value::Object(
            v.iter()
                .map(|(k, v)| Ok((k.as_str().to_owned(), json_body(v)?)))
                .collect::<Result<_, ()>>()?,
        ),
        // No byte payloads or unselected future AnyValue variants enter this sink.
        _ => return Err(()),
    })
}
fn attribute(value: &AttributeValue) -> Option<Value> {
    Some(match value {
        AttributeValue::Bool(v) => json!(v),
        AttributeValue::I64(v) => json!(v),
        AttributeValue::F64(v) => json!(v),
        AttributeValue::String(v) => json!(v.as_str()),
        _ => return None,
    })
}
fn resource(resource: &Resource) -> Value {
    Value::Object(
        resource
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_owned(), attribute(v)?)))
            .collect(),
    )
}
fn time(value: SystemTime) -> String {
    value
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}
impl LocalLog {
    pub(super) fn json(&self) -> Result<(&'static str, Value), ()> {
        let request = self.record.event_name() == Some("generation.request.finished")
            || self.record.event_name() == Some("gateway.request.finished");
        let mut value = if request {
            self.record.body().ok_or(()).and_then(json_body)?
        } else {
            json!({})
        };
        let object = value.as_object_mut().ok_or(())?;
        let context = self.record.trace_context();
        let attributes = self
            .record
            .attributes_iter()
            .map(|(k, v)| Ok((k.as_str().to_owned(), json_body(v)?)))
            .collect::<Result<serde_json::Map<_, _>, ()>>()?;
        object.insert("telemetry".into(),json!({
            "event_name":self.record.event_name(),"resource":resource(&self.resource),
            "scope":self.scope.name(),"severity":self.record.severity_text(),
            "timestamp_unix_nano":self.record.timestamp().map(time),
            "observed_timestamp_unix_nano":self.record.observed_timestamp().map(time),
            "trace_id":context.map(|c|c.trace_id.to_string()),"span_id":context.map(|c|c.span_id.to_string()),
            "attributes":attributes,"body_encoding":{"null_map_members":"omitted","wide_unsigned_integers":"decimal_string"}
        }));
        Ok((if request { "request" } else { "log" }, value))
    }
}
impl LocalSpan {
    pub(super) fn json(&self) -> Value {
        let span = &self.span;
        let attributes: serde_json::Map<_, _> = span
            .attributes
            .iter()
            .filter_map(|kv| Some((kv.key.as_str().to_owned(), attribute(&kv.value)?)))
            .collect();
        let (status, error_type) = match &span.status {
            opentelemetry::trace::Status::Ok => ("ok", None),
            opentelemetry::trace::Status::Unset => ("unset", None),
            opentelemetry::trace::Status::Error { description } => {
                ("error", Some(description.as_ref()))
            }
        };
        json!({"name":span.name,"trace_id":span.span_context.trace_id().to_string(),"span_id":span.span_context.span_id().to_string(),"parent_span_id":span.parent_span_id.to_string(),"start_unix_nano":time(span.start_time),"end_unix_nano":time(span.end_time),"status":status,"error_type":error_type,"attributes":attributes,"resource":resource(&self.resource),"scope":span.instrumentation_scope.name(),"dropped_attributes":span.dropped_attributes_count})
    }
}
