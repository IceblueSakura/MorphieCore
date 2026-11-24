//! Bounded OpenAI Images preview/completed SSE stream decoding and encoding.
//! Sources: https://developers.openai.com/api/reference/resources/images/generation-streaming-events
//! https://developers.openai.com/api/reference/resources/images/edit-streaming-events
use crate::{
    CodecError,
    semantic::{
        task::image_generation::{
            ImageBackgroundRequest, ImageData, ImageQualityRequest, ImageSizeRequest,
            ImageStreamEvent, ImageStreamSettings, MAX_PARTIAL_IMAGES,
        },
        value::ImageFormat,
    },
};
use bytes::Bytes;
use serde_json::{Map, Value, json};

pub struct StreamDecoder {
    is_edit: bool,
    requested_partial_limit: Option<u8>,
    expected_format: ImageFormat,
    per_image_limit: usize,
    max_total_decoded_bytes: usize,
    decoded_bytes: usize,
    next_min_index: u8,
    remaining_events: usize,
    completed: bool,
    poisoned: bool,
}

impl StreamDecoder {
    pub fn new(
        is_edit: bool,
        requested_partial_limit: Option<u8>,
        expected_format: ImageFormat,
        per_image_limit: usize,
        max_total_decoded_bytes: usize,
        max_events: usize,
    ) -> Self {
        Self {
            is_edit,
            requested_partial_limit,
            expected_format,
            per_image_limit,
            max_total_decoded_bytes,
            decoded_bytes: 0,
            next_min_index: 0,
            remaining_events: max_events.min(65536),
            completed: false,
            poisoned: false,
        }
    }

    pub fn push(&mut self, value: &Value) -> Result<ImageStreamEvent, CodecError> {
        if self.poisoned {
            return Err(CodecError::invalid());
        }
        let result = self.push_inner(value);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn decode_image(&mut self, value: &Value) -> Result<ImageData, CodecError> {
        let encoded = super::string(value)?;
        let remaining_total = self
            .max_total_decoded_bytes
            .saturating_sub(self.decoded_bytes);
        let budget = self.per_image_limit.min(remaining_total);
        if encoded.len() > budget.div_ceil(3) * 4 {
            return Err(CodecError::Limit);
        }
        let data = ImageData::new(encoded).map_err(|_| CodecError::invalid())?;
        if data.decoded_bytes() > self.per_image_limit {
            return Err(CodecError::Limit);
        }
        self.decoded_bytes = self
            .decoded_bytes
            .checked_add(data.decoded_bytes())
            .filter(|n| *n <= self.max_total_decoded_bytes)
            .ok_or(CodecError::Limit)?;
        Ok(data)
    }

    fn parse_settings(&self, map: &Map<String, Value>) -> Result<ImageStreamSettings, CodecError> {
        let format = super::format(super::required(map, "output_format")?)?;
        if format != self.expected_format {
            return Err(CodecError::invalid());
        }
        let settings = ImageStreamSettings {
            format,
            size: super::size_request_str(super::string(super::required(map, "size")?)?)?,
            quality: super::quality_request_str(super::string(super::required(map, "quality")?)?)?,
            background: super::background_request_str(super::string(super::required(
                map,
                "background",
            )?)?)?,
        };
        settings.validate().map_err(|_| CodecError::invalid())?;
        Ok(settings)
    }

    fn push_inner(&mut self, value: &Value) -> Result<ImageStreamEvent, CodecError> {
        if self.completed || self.remaining_events == 0 {
            return Err(CodecError::invalid());
        }
        self.remaining_events -= 1;
        let map = value.as_object().ok_or_else(CodecError::invalid)?;
        let kind = super::string(super::required(map, "type")?)?;
        let partial_kind = if self.is_edit {
            "image_edit.partial_image"
        } else {
            "image_generation.partial_image"
        };
        let completed_kind = if self.is_edit {
            "image_edit.completed"
        } else {
            "image_generation.completed"
        };
        if kind == partial_kind {
            if self.requested_partial_limit == Some(0) {
                return Err(CodecError::invalid());
            }
            let effective_limit = self
                .requested_partial_limit
                .unwrap_or(MAX_PARTIAL_IMAGES)
                .min(MAX_PARTIAL_IMAGES);
            let index = u8::try_from(super::number(super::required(map, "partial_image_index")?)?)
                .map_err(|_| CodecError::invalid())?;
            if index >= effective_limit || index < self.next_min_index {
                return Err(CodecError::invalid());
            }
            self.next_min_index = index + 1;
            let event_time = super::number(super::required(map, "created_at")?)?;
            let settings = self.parse_settings(map)?;
            let data = self.decode_image(super::required(map, "b64_json")?)?;
            Ok(ImageStreamEvent::Preview {
                output_slot: 0,
                preview_index: index,
                data,
                event_time,
                settings,
            })
        } else if kind == completed_kind {
            let event_time = super::number(super::required(map, "created_at")?)?;
            let settings = self.parse_settings(map)?;
            let safety_identifier =
                super::optional(map, "safety_identifier", |v| Ok(super::string(v)?.into()))?;
            if safety_identifier
                .value()
                .is_some_and(|s: &String| s.chars().count() > 128)
            {
                return Err(CodecError::invalid());
            }
            let usage = super::usage(super::required(map, "usage")?)?;
            let data = self.decode_image(super::required(map, "b64_json")?)?;
            self.completed = true;
            Ok(ImageStreamEvent::Completed {
                data,
                event_time,
                settings,
                safety_identifier,
                usage: Some(Box::new(usage)),
            })
        } else {
            Err(CodecError::invalid())
        }
    }

    pub fn finish(&self) -> Result<(), CodecError> {
        if self.poisoned || !self.completed {
            return Err(CodecError::invalid());
        }
        Ok(())
    }
}

pub fn encode_sse_event(event: &ImageStreamEvent, is_edit: bool) -> Result<Bytes, CodecError> {
    let encode_settings = |map: &mut Map<String, Value>, settings: &ImageStreamSettings| {
        map.insert(
            "output_format".into(),
            super::format_value(&settings.format),
        );
        map.insert(
            "size".into(),
            match settings.size {
                ImageSizeRequest::Auto => json!("auto"),
                ImageSizeRequest::Exact(s) => json!(format!("{}x{}", s.width, s.height)),
            },
        );
        map.insert(
            "quality".into(),
            match settings.quality {
                ImageQualityRequest::Auto => json!("auto"),
                ImageQualityRequest::Exact(q) => json!(Into::<&'static str>::into(q)),
            },
        );
        map.insert(
            "background".into(),
            match settings.background {
                ImageBackgroundRequest::Auto => json!("auto"),
                ImageBackgroundRequest::Exact(b) => json!(Into::<&'static str>::into(b)),
            },
        );
    };
    let (name, map) = match event {
        ImageStreamEvent::Preview {
            preview_index,
            data,
            event_time,
            settings,
            ..
        } => {
            settings.validate().map_err(|_| CodecError::invalid())?;
            let kind = if is_edit {
                "image_edit.partial_image"
            } else {
                "image_generation.partial_image"
            };
            let mut map = Map::new();
            map.insert("type".into(), json!(kind));
            map.insert("partial_image_index".into(), json!(preview_index));
            map.insert("b64_json".into(), json!(data.as_base64()));
            map.insert("created_at".into(), json!(event_time));
            encode_settings(&mut map, settings);
            (kind, map)
        }
        ImageStreamEvent::Completed {
            data,
            event_time,
            settings,
            safety_identifier,
            usage,
        } => {
            settings.validate().map_err(|_| CodecError::invalid())?;
            if let Some(usage) = usage {
                usage.validate().map_err(|_| CodecError::invalid())?;
                if !crate::image::projection::standard_tokens_representable(usage) {
                    return Err(CodecError::invalid());
                }
            }
            let kind = if is_edit {
                "image_edit.completed"
            } else {
                "image_generation.completed"
            };
            let mut map = Map::new();
            map.insert("type".into(), json!(kind));
            map.insert("b64_json".into(), json!(data.as_base64()));
            map.insert("created_at".into(), json!(event_time));
            encode_settings(&mut map, settings);
            super::put(&mut map, "safety_identifier", safety_identifier, |v| {
                json!(v)
            });
            if let Some(usage) = usage {
                map.insert("usage".into(), super::usage_value(usage));
            }
            (kind, map)
        }
    };
    let encoded = serde_json::to_string(&Value::Object(map)).map_err(|_| CodecError::invalid())?;
    Ok(Bytes::from(format!("event: {name}\ndata: {encoded}\n\n")))
}
