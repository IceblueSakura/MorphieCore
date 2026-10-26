//! Protocol error translation over the shared strict JSON value parser.
use crate::protocol::CodecError;
use crate::protocol::failure::ShapeReason;
use crate::semantic::value::{JsonError, JsonLimits, parse_json};

pub(crate) fn decode(input: &[u8]) -> Result<serde_json::Value, CodecError> {
    parse_json(input, JsonLimits::ENVELOPE).map_err(|error| match error {
        JsonError::Limit(resource) => CodecError::from(JsonError::Limit(resource)),
        JsonError::Invalid => CodecError::Invalid(ShapeReason::Json),
    })
}
pub(crate) fn decode_image_response(input: &[u8]) -> Result<serde_json::Value, CodecError> {
    crate::semantic::value::parse_image_response_json(input).map_err(|error| match error {
        JsonError::Limit(resource) => CodecError::from(JsonError::Limit(resource)),
        JsonError::Invalid => CodecError::Invalid(ShapeReason::Json),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn duplicate_keys_are_compared_after_unescaping_at_every_level() {
        for input in [
            r#"{"a":1,"a":2}"#,
            r#"{"a":1,"\u0061":2}"#,
            r#"{"items":[{"a":1,"a":1}]}"#,
            r#"{"outer":{"a":null,"a":false}}"#,
        ] {
            assert_eq!(
                decode(input.as_bytes()),
                Err(CodecError::Invalid(ShapeReason::Json))
            );
        }
        assert_eq!(
            decode(br#"{"a":{"x":1},"b":{"x":2}}"#).unwrap(),
            json!({"a":{"x":1},"b":{"x":2}})
        );
        // Embedded tool arguments remain opaque text, not a second JSON document to parse.
        assert_eq!(
            decode(br#""{\"a\":1,\"a\":2}""#).unwrap(),
            json!("{\"a\":1,\"a\":2}")
        );
    }

    #[test]
    fn malformed_utf8_json_and_trailing_data_are_rejected_without_echoing_payload() {
        for input in [
            b"\xff".as_slice(),
            b"{} []",
            b"null x",
            b"{\"a\":}",
            b"[1,]",
            b"\xef\xbb\xbf{}",
        ] {
            let error = decode(input).unwrap_err();
            assert_eq!(error, CodecError::Invalid(ShapeReason::Json));
            assert_eq!(error.to_string(), "invalid JSON");
        }
        assert_eq!(
            decode(b" \r\n {\"n\":18446744073709551615,\"text\":\"\\u4f60\\u597d\"} \t").unwrap(),
            json!({"n":u64::MAX,"text":"你好"})
        );
        assert_eq!(
            decode(b"[-9223372036854775808,1.25,-0.0,true,false,null]").unwrap(),
            json!([i64::MIN, 1.25, -0.0, true, false, null])
        );
    }

    #[test]
    fn exact_numbers_do_not_turn_reserved_keys_into_numeric_values() {
        let input = br#"{"big":18446744073709551616001,"decimal":0.123456789012345678901,"nested":{"$serde_json::private::Number":"1"}}"#;
        let value = decode(input).unwrap();
        assert_eq!(value["big"].to_string(), "18446744073709551616001");
        assert_eq!(value["decimal"].to_string(), "0.123456789012345678901");
        assert_eq!(value["nested"]["$serde_json::private::Number"], "1");
        assert!(value["nested"].is_object());
        assert_eq!(
            decode(br#"{"$serde_json::private::Number":"1","$serde_json::private::Number":"2"}"#),
            Err(CodecError::Invalid(ShapeReason::Json))
        );
    }

    #[test]
    fn raw_byte_budget_counts_whitespace_before_parsing() {
        for (limits, parse) in [
            (JsonLimits::ENVELOPE, decode as fn(&[u8]) -> _),
            (JsonLimits::IMAGE_RESPONSE, decode_image_response),
        ] {
            let mut input = vec![b' '; limits.bytes];
            input[..4].copy_from_slice(b"null");
            assert_eq!(parse(&input).unwrap(), Value::Null);
            input.push(b' ');
            assert_eq!(
                parse(&input),
                Err(CodecError::Resource(
                    crate::protocol::failure::Resource::Bytes
                ))
            );
        }
    }

    #[test]
    fn container_depth_is_bounded_before_building_nested_values() {
        for (open, close) in [("[", "]"), ("{\"child\":", "}")] {
            for (depth, accepted) in [(64, true), (65, false)] {
                let input = format!("{}0{}", open.repeat(depth), close.repeat(depth));
                let result = decode(input.as_bytes());
                if accepted {
                    assert!(result.is_ok());
                } else {
                    assert_eq!(
                        result.err(),
                        Some(CodecError::Resource(
                            crate::protocol::failure::Resource::Depth
                        ))
                    );
                }
            }
        }
    }

    #[test]
    fn node_budget_counts_values_and_object_keys() {
        let nodes = JsonLimits::ENVELOPE.nodes;
        let input = format!("[{}]", vec!["0"; nodes - 1].join(","));
        assert!(decode(input.as_bytes()).is_ok());
        let input = format!("[{}]", vec!["0"; nodes].join(","));
        assert_eq!(
            decode(input.as_bytes()).err(),
            Some(CodecError::Resource(
                crate::protocol::failure::Resource::Nodes
            ))
        );
        for (entries, accepted) in [(nodes / 2 - 1, true), (nodes / 2, false)] {
            let input = format!(
                "{{{}}}",
                (0..entries)
                    .map(|i| format!("\"k{i}\":0"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let result = decode(input.as_bytes());
            if accepted {
                assert!(result.is_ok());
            } else {
                assert_eq!(
                    result.err(),
                    Some(CodecError::Resource(
                        crate::protocol::failure::Resource::Nodes
                    ))
                );
            }
        }
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use crate::{
        protocol::failure::Resource,
        semantic::value::{JsonLimits, parse_json},
    };
    #[test]
    fn byte_depth_and_node_bounds_keep_their_responsible_resource() {
        for (input, limits, resource) in [
            (
                b"[0]".as_slice(),
                JsonLimits {
                    bytes: 2,
                    ..JsonLimits::STRUCTURED
                },
                Resource::Bytes,
            ),
            (
                b"[[0]]".as_slice(),
                JsonLimits {
                    depth: 1,
                    ..JsonLimits::STRUCTURED
                },
                Resource::Depth,
            ),
            (
                b"[0,0]".as_slice(),
                JsonLimits {
                    nodes: 2,
                    ..JsonLimits::STRUCTURED
                },
                Resource::Nodes,
            ),
        ] {
            assert_eq!(
                CodecError::from(parse_json(input, limits).unwrap_err()),
                CodecError::Resource(resource)
            );
        }
    }
}
