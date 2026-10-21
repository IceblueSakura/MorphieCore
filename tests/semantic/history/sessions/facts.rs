//! Deliberately small semantic oracle, not a serialization of production IR.
use morphiecore::semantic::task::generation::*;
use serde_json::{Value, json};

fn replay(value: Option<&ReplayValue>) -> Value {
    value.map_or(
        Value::Null,
        |v| json!({"format":format!("{:?}", v.format()), "token":v.replay_token()}),
    )
}

pub(super) fn response(response: &GenerationResponse) -> Value {
    let items: Vec<_> = response.items().iter().map(|(_, item)| match item {
        Item::Message(m) => json!({"kind":"message", "role":format!("{:?}",m.role),
            "status":format!("{:?}",m.status), "phase":m.phase.map(|p|format!("{p:?}")),
            "parts":m.parts.iter().map(|p|match &p.content {
                ContentPart::Text(t) => json!({"text":t.as_str(), "replay":replay(p.replay.as_ref())}),
                other => panic!("unselected message part {other:?}"),
            }).collect::<Vec<_>>() }),
        Item::Reasoning(r) => json!({"kind":"reasoning", "status":format!("{:?}",r.status),
            "parts":r.parts.iter().map(|(_,p)|match p {
                ReasoningContent::Summary(t) => json!({"summary":t.as_str()}),
                ReasoningContent::Text(t) => json!({"text":t.as_str()}),
            }).collect::<Vec<_>>(), "replay":replay(r.replay.as_ref())}),
        Item::ToolCall(c) => json!({"kind":"call", "id":c.call_id.as_str(),"name":c.name.as_str(),
            "status":format!("{:?}",c.status),"arguments":match &c.arguments {
                ToolArguments::Raw(s) => json!({"raw":s}),
                // A string comparison also checks structured object order and exact numbers.
                ToolArguments::Structured(v) => json!({"json":v.value().to_string()}),
                ToolArguments::StructuredPartial(s) => json!({"partial_json":s}),
            }, "replay":replay(c.replay.as_ref())}),
        other => panic!("unselected response item {other:?}"),
    }).collect();
    let usage = response.usage_reports().operation().map(|r| {
        let u = r.counts;
        let accounting = match r.accounting {
            UsageAccounting::Direct => json!({"kind":"direct"}),
            UsageAccounting::SplitGeneration {
                tool_input_tokens,
                tool_input_relation,
                total_relation,
            } => json!({"kind":"split", "tool_input":tool_input_tokens,
                    "tool_relation":format!("{tool_input_relation:?}"),
                    "total_relation":format!("{total_relation:?}")}),
        };
        json!({"input":u.input_tokens,"output":u.output_tokens,"total":u.total_tokens,
            "reasoning":u.reasoning_tokens,"cached":u.cached_input_tokens,
            "cache_write":u.input_cache_write_tokens,
            "input_relation":format!("{:?}",u.input_relation),
            "output_relation":format!("{:?}",u.output_relation),
            "total_relation":format!("{:?}",u.total_relation),
            "stage":format!("{:?}",r.stage),"accounting":accounting,
            "defaulted":r.details.defaulted.iter().map(|c|format!("{c:?}")).collect::<Vec<_>>()})
    });
    json!({"outcome":format!("{:?}",response.outcome()),
        "progress":format!("{:?}",response.progress()),
        "completion":response.details().completion.as_ref().map(|c|format!("{c:?}")),
        "incomplete":response.details().incomplete.as_ref().map(|c|format!("{c:?}")),
        "items":items,"usage":usage})
}
