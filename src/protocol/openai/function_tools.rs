//! Closed function/custom tool syntax; active orchestration features are outside this profile.
use crate::{
    protocol::{
        failure::CarrierReason,
        openai::{
            CodecError, GenerationApi,
            content::call::{reject_chat_orchestration, strict_default},
            shape::*,
        },
    },
    semantic::task::generation::*,
};
use serde_json::{Map, Value, json};
fn dispatch(o: &Map<String, Value>) -> Result<ToolDispatch, CodecError> {
    // Definition flags are non-nullable; call-item `async` has its own schema.
    // https://github.com/openai/openai-python/blob/be9d66628ad7377bd36fe5a76ae6d735843f0e76/src/openai/types/responses/function_tool_param.py
    let flag = |key| match o.get(key) {
        None | Some(Value::Bool(false)) => Ok(false),
        Some(Value::Bool(true)) => Ok(true),
        _ => Err(CodecError::invalid()),
    };
    let allowed_callers = match o.get("allowed_callers") {
        None | Some(Value::Null) => None,
        Some(Value::Array(values)) if values == &vec![json!("direct")] => None,
        Some(Value::Array(values)) => {
            if values.is_empty() || values.len() > 2 {
                return Err(CodecError::invalid());
            }
            let mut callers = vec![];
            for value in values {
                let mode = match value.as_str() {
                    Some("direct") => CallerMode::Direct,
                    Some("programmatic") => CallerMode::Programmatic,
                    _ => {
                        return Err(CodecError::invalid());
                    }
                };
                if callers.contains(&mode) {
                    return Err(CodecError::invalid());
                }
                callers.push(mode);
            }
            Some(callers)
        }
        _ => {
            return Err(CodecError::invalid());
        }
    };
    Ok(ToolDispatch {
        async_call: flag("async")?,
        defer_loading: flag("defer_loading")?,
        allowed_callers,
    })
}
fn write_dispatch(dispatch: &ToolDispatch, o: &mut Map<String, Value>) {
    if dispatch.async_call {
        o.insert("async".into(), json!(true));
    }
    if dispatch.defer_loading {
        o.insert("defer_loading".into(), json!(true));
    }
    if let Some(callers) = &dispatch.allowed_callers {
        o.insert(
            "allowed_callers".into(),
            json!(
                callers
                    .iter()
                    .map(|c| match c {
                        CallerMode::Direct => "direct",
                        CallerMode::Programmatic => "programmatic",
                    })
                    .collect::<Vec<_>>()
            ),
        );
    }
}
pub(super) fn read_settings(
    o: &Map<String, Value>,
    profile: GenerationApi,
    settings: &mut GenerationSettings,
    direction: DecodeDirection,
) -> Result<(), CodecError> {
    // The pinned create types mark the tool containers non-nullable, and the
    // Chat create type marks parallel_tool_calls non-nullable too: an explicit
    // null never means "no tools".
    for key in ["tools", "tool_choice"] {
        if o.get(key).is_some_and(Value::is_null) {
            return Err(CodecError::invalid());
        }
    }
    if profile == GenerationApi::Chat && o.get("parallel_tool_calls").is_some_and(Value::is_null) {
        return Err(CodecError::invalid());
    }
    settings.tools = o
        .get("tools")
        .filter(|v| !v.is_null())
        .map(|v| {
            let a = v.as_array().ok_or(CodecError::invalid())?;
            if a.len() > MAX_TOOLS {
                return Err(CodecError::Limit);
            }
            a.iter()
                .map(|v| read_tool(object(v)?, profile, direction))
                .collect::<Result<Vec<_>, CodecError>>()
        })
        .transpose()?;
    settings.tool_choice = o
        .get("tool_choice")
        .filter(|v| !v.is_null())
        .map(|v| read_choice(v, profile, direction))
        .transpose()?;
    settings.parallel_tool_calls = nullable_bool(o, "parallel_tool_calls")?;
    settings.validate()?;
    Ok(())
}
pub(super) fn read_tool(
    o: &Map<String, Value>,
    profile: GenerationApi,
    direction: DecodeDirection,
) -> Result<ToolDefinition, CodecError> {
    if string(o, "type")? == "namespace" && profile == GenerationApi::Responses {
        direction.fields(o, &["type", "name", "description", "tools"])?;
        let tools = o
            .get("tools")
            .and_then(Value::as_array)
            .ok_or(CodecError::invalid())?;
        if tools.is_empty() || tools.len() > MAX_TOOLS {
            return Err(CodecError::Limit);
        }
        let mut leaves = Vec::with_capacity(tools.len());
        for tool in tools {
            let tool = object(tool)?;
            if !matches!(string(tool, "type")?, "function" | "custom") {
                return Err(CodecError::Unsupported(CarrierReason::Unavailable));
            }
            leaves.push(read_tool(tool, profile, direction)?);
        }
        return Ok(ToolDefinition::Namespace(ToolNamespace {
            name: text(string(o, "name")?, "namespace", 128)?,
            description: raw_string(o, "description")?,
            tools: leaves,
        }));
    }
    if string(o, "type")? == "custom" {
        let o = if profile == GenerationApi::Chat {
            reject_chat_orchestration(o)?;
            direction.fields(o, &["type", "custom"])?;
            let custom = object(o.get("custom").ok_or(CodecError::invalid())?)?;
            reject_chat_orchestration(custom)?;
            custom
        } else {
            o
        };
        direction.fields(
            o,
            &[
                "type",
                "name",
                "description",
                "format",
                "async",
                "defer_loading",
                "allowed_callers",
            ],
        )?;
        let dispatch = if profile == GenerationApi::Chat {
            ToolDispatch::default()
        } else {
            dispatch(o)?
        };
        let format = o
            .get("format")
            .map(|v| {
                let f = object(v)?;
                Ok::<_, CodecError>(match string(f, "type")? {
                    "text" => {
                        direction.fields(f, &["type"])?;
                        CustomFormat::Text
                    }
                    "grammar" => {
                        let grammar = if profile == GenerationApi::Chat {
                            direction.fields(f, &["type", "grammar"])?;
                            let grammar = object(f.get("grammar").ok_or(CodecError::invalid())?)?;
                            direction.fields(grammar, &["syntax", "definition"])?;
                            grammar
                        } else {
                            direction.fields(f, &["type", "syntax", "definition"])?;
                            f
                        };
                        CustomFormat::Grammar {
                            syntax: match string(grammar, "syntax")? {
                                "lark" => GrammarSyntax::Lark,
                                "regex" => GrammarSyntax::Regex,
                                _ => {
                                    return Err(CodecError::invalid());
                                }
                            },
                            definition: text(
                                string(grammar, "definition")?,
                                "grammar",
                                MAX_TEXT_BYTES,
                            )?,
                        }
                    }
                    _ => {
                        return Err(CodecError::invalid());
                    }
                })
            })
            .transpose()?;
        if profile == GenerationApi::Chat && o.get("description").is_some_and(Value::is_null) {
            return Err(CodecError::invalid());
        }
        return Ok(ToolDefinition::Custom(CustomTool {
            name: text(string(o, "name")?, "custom tool name", 128)?,
            description: o
                .get("description")
                .filter(|v| !v.is_null())
                .map(|_| raw_string(o, "description"))
                .transpose()?,
            format,
            dispatch,
        }));
    }
    if string(o, "type")? != "function" {
        return Err(CodecError::Unsupported(CarrierReason::Tools));
    }
    let f = if profile == GenerationApi::Chat {
        reject_chat_orchestration(o)?;
        direction.fields(o, &["type", "function"])?;
        object(o.get("function").ok_or(CodecError::invalid())?)?
    } else {
        o
    };
    direction.fields(
        f,
        if profile == GenerationApi::Chat {
            &["name", "description", "parameters", "strict"]
        } else {
            &[
                "type",
                "name",
                "description",
                "parameters",
                "strict",
                "output_schema",
                "async",
                "defer_loading",
                "allowed_callers",
            ]
        },
    )?;
    let dispatch = if profile == GenerationApi::Chat {
        reject_chat_orchestration(f)?;
        ToolDispatch::default()
    } else {
        dispatch(f)?
    };
    if profile == GenerationApi::Chat
        && (["description", "parameters"]
            .iter()
            .any(|key| f.get(*key).is_some_and(Value::is_null))
            || !valid_chat_function_name(string(f, "name")?))
    {
        return Err(CodecError::invalid());
    }
    Ok(ToolDefinition::Function(FunctionTool {
        name: text(string(f, "name")?, "function name", 128)?,
        description: f
            .get("description")
            .filter(|v| !v.is_null())
            .map(|_| raw_string(f, "description"))
            .transpose()?,
        parameters: f
            .get("parameters")
            .filter(|v| !v.is_null())
            .cloned()
            .map(Into::into),
        strict: match nullable_bool(f, "strict")? {
            Some(v) => FunctionStrictness::Explicit(v),
            None => FunctionStrictness::Omitted(strict_default(profile)),
        },
        output_schema: if profile == GenerationApi::Responses {
            f.get("output_schema")
                .filter(|v| !v.is_null())
                .cloned()
                .map(Into::into)
        } else {
            None
        },
        dispatch,
        cache: None,
        input_examples: None,
        eager_input: None,
    }))
}
fn reference(
    v: &Value,
    profile: GenerationApi,
    direction: DecodeDirection,
) -> Result<ToolReference, CodecError> {
    let o = object(v)?;
    if profile == GenerationApi::Chat {
        let (kind, field) = match string(o, "type")? {
            "function" => (ToolKind::Function, "function"),
            "custom" => (ToolKind::Custom, "custom"),
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            }
        };
        reject_chat_orchestration(o)?;
        direction.fields(o, &["type", field])?;
        let f = object(o.get(field).ok_or(CodecError::invalid())?)?;
        reject_chat_orchestration(f)?;
        direction.fields(f, &["name"])?;
        return Ok(ToolReference {
            kind,
            name: text(string(f, "name")?, "tool reference", 128)?,
            namespace: None,
        });
    }
    direction.fields(o, &["type", "name"])?;
    Ok(ToolReference {
        kind: match string(o, "type")? {
            "function" => ToolKind::Function,
            "custom" => ToolKind::Custom,
            _ => {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            }
        },
        name: text(string(o, "name")?, "tool reference", 128)?,
        namespace: None,
    })
}
fn read_choice(
    v: &Value,
    profile: GenerationApi,
    direction: DecodeDirection,
) -> Result<ToolChoice, CodecError> {
    Ok(match v.as_str() {
        Some("auto") => ToolChoice::Auto,
        Some("none") => ToolChoice::None,
        Some("required") => ToolChoice::Required,
        Some(_) => {
            return Err(CodecError::invalid());
        }
        None => {
            let o = object(v)?;
            if string(o, "type")? == "allowed_tools" {
                let allowed = if profile == GenerationApi::Chat {
                    direction.fields(o, &["type", "allowed_tools"])?;
                    let allowed = object(o.get("allowed_tools").ok_or(CodecError::invalid())?)?;
                    direction.fields(allowed, &["mode", "tools"])?;
                    allowed
                } else {
                    direction.fields(o, &["type", "mode", "tools"])?;
                    o
                };
                let required = match string(allowed, "mode")? {
                    "auto" => false,
                    "required" => true,
                    _ => {
                        return Err(CodecError::invalid());
                    }
                };
                let a = allowed
                    .get("tools")
                    .and_then(Value::as_array)
                    .ok_or(CodecError::invalid())?;
                if a.len() > MAX_TOOLS {
                    return Err(CodecError::Limit);
                }
                ToolChoice::Allowed {
                    required,
                    tools: a
                        .iter()
                        .map(|v| reference(v, profile, direction))
                        .collect::<Result<_, _>>()?,
                }
            } else {
                let r = reference(v, profile, direction)?;
                match r.kind {
                    ToolKind::Function => ToolChoice::Specific(r.name),
                    ToolKind::Custom => ToolChoice::Custom(r.name),
                }
            }
        }
    })
}
pub(super) fn decode(
    r: GenerationRequest,
    o: &Map<String, Value>,
    profile: GenerationApi,
    direction: DecodeDirection<'_>,
) -> Result<GenerationRequest, CodecError> {
    let mut s = r.settings().clone();
    read_settings(o, profile, &mut s, direction)?;
    Ok(r.with_settings(s)?)
}
pub(super) fn encode(
    r: &GenerationRequest,
    profile: GenerationApi,
    o: &mut Map<String, Value>,
) -> Result<(), CodecError> {
    write_settings(r.settings(), profile, o)
}
pub(super) fn write_settings(
    s: &GenerationSettings,
    profile: GenerationApi,
    o: &mut Map<String, Value>,
) -> Result<(), CodecError> {
    if let Some(tools) = &s.tools {
        o.insert(
            "tools".into(),
            json!(
                tools
                    .iter()
                    .map(|t| write_tool(t, profile))
                    .collect::<Result<Vec<_>, _>>()?
            ),
        );
    }
    if let Some(c) = &s.tool_choice {
        o.insert("tool_choice".into(), write_choice(c, profile)?);
    }
    if let Some(v) = s.parallel_tool_calls {
        o.insert("parallel_tool_calls".into(), json!(v));
    }
    Ok(())
}
pub(super) fn valid_chat_function_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
pub(super) fn write_tool(t: &ToolDefinition, profile: GenerationApi) -> Result<Value, CodecError> {
    Ok(match t {
        ToolDefinition::Namespace(_) if profile == GenerationApi::Chat => {
            return Err(CodecError::Unsupported(CarrierReason::Tools));
        }
        ToolDefinition::Namespace(group) => json!({
            "type":"namespace","name":group.name.as_str(),"description":group.description,
            "tools":group.tools.iter().map(|t| write_tool(t, profile)).collect::<Result<Vec<_>, _>>()?
        }),
        ToolDefinition::Function(t) => {
            if profile == GenerationApi::Chat && !valid_chat_function_name(t.name.as_str()) {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            }
            if t.input_examples.is_some() {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            }
            let mut f = json!({"name":t.name.as_str()});
            if let Some(d) = &t.description {
                f["description"] = json!(d);
            }
            if let Some(p) = &t.parameters {
                p.validate()?;
                f["parameters"] = p.root().clone();
            }
            if let Some(p) = &t.output_schema {
                p.validate()?;
                f["output_schema"] = p.root().clone();
            }
            if let FunctionStrictness::Explicit(b) = t.strict {
                f["strict"] = json!(b);
            } else if profile == GenerationApi::Responses
                && t.strict == FunctionStrictness::Omitted(StrictDefault::NonStrict)
            {
                f["strict"] = json!(false);
            }
            if profile == GenerationApi::Responses {
                write_dispatch(&t.dispatch, f.as_object_mut().expect("object"));
            }
            if profile == GenerationApi::Chat {
                json!({"type":"function","function":f})
            } else {
                f["type"] = json!("function");
                f
            }
        }
        ToolDefinition::Custom(t) => {
            let mut v = json!({"name":t.name.as_str()});
            if let Some(d) = &t.description {
                v["description"] = json!(d);
            }
            if let Some(format) = &t.format {
                v["format"] = match format {
                    CustomFormat::Text => json!({"type":"text"}),
                    CustomFormat::Grammar { syntax, definition } => {
                        let syntax = match syntax {
                            GrammarSyntax::Lark => "lark",
                            GrammarSyntax::Regex => "regex",
                        };
                        let grammar = json!({
                            "syntax": syntax,
                            "definition": definition.as_str(),
                        });
                        if profile == GenerationApi::Chat {
                            json!({"type":"grammar","grammar":grammar})
                        } else {
                            let mut grammar = grammar;
                            grammar["type"] = json!("grammar");
                            grammar
                        }
                    }
                };
            }
            if profile == GenerationApi::Chat {
                json!({"type":"custom","custom":v})
            } else {
                write_dispatch(&t.dispatch, v.as_object_mut().expect("object"));
                v["type"] = json!("custom");
                v
            }
        }
    })
}
pub(super) fn write_choice(c: &ToolChoice, profile: GenerationApi) -> Result<Value, CodecError> {
    Ok(match c {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Specific(n) if profile == GenerationApi::Chat => {
            json!({"type":"function","function":{"name":n.as_str()}})
        }
        ToolChoice::Specific(n) => json!({"type":"function","name":n.as_str()}),
        ToolChoice::Custom(n) if profile == GenerationApi::Chat => {
            json!({"type":"custom","custom":{"name":n.as_str()}})
        }
        ToolChoice::Custom(n) => json!({"type":"custom","name":n.as_str()}),
        ToolChoice::Qualified(_) => {
            return Err(CodecError::Unsupported(CarrierReason::Tools));
        }
        ToolChoice::Allowed { required, tools } => {
            if tools.iter().any(|r| r.namespace.is_some()) {
                return Err(CodecError::Unsupported(CarrierReason::Tools));
            }
            let refs: Vec<_> = tools
                .iter()
                .map(|r| {
                    if profile == GenerationApi::Chat {
                        match r.kind {
                            ToolKind::Function => {
                                json!({"type":"function","function":{"name":r.name.as_str()}})
                            }
                            ToolKind::Custom => {
                                json!({"type":"custom","custom":{"name":r.name.as_str()}})
                            }
                        }
                    } else {
                        write_reference(r)
                    }
                })
                .collect();
            let mut allowed = json!({"mode":if *required{"required"}else{"auto"},"tools":refs});
            if profile == GenerationApi::Chat {
                json!({"type":"allowed_tools","allowed_tools":allowed})
            } else {
                allowed["type"] = json!("allowed_tools");
                allowed
            }
        }
    })
}
fn write_reference(r: &ToolReference) -> Value {
    let kind = match r.kind {
        ToolKind::Function => "function",
        ToolKind::Custom => "custom",
    };
    json!({ "type": kind, "name": r.name.as_str() })
}
