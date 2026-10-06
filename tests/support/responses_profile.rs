//! Synthetic, independently authored Responses wire oracle. No provider or credentials.
use serde_json::{Value, json};
pub fn tools() -> Value {
    json!([
        {"type":"function","name":"lookup","parameters":{"type":"object","properties":{"n":{"type":"integer"}},"required":["n"],"additionalProperties":false},"strict":true,"output_schema":{"type":"string"}},
        {"type":"custom","name":"sql","description":"Synthetic text only","format":{"type":"grammar","syntax":"regex","definition":"SELECT [0-9]+"}}
    ])
}
pub fn request(stream: bool) -> Value {
    json!({
        "model":"fixture-model","instructions":"Answer precisely","input":"hello 🧪","stream":stream,"store":false,"background":false,"previous_response_id":null,"conversation":null,
        "tools":tools(),"tool_choice":{"type":"allowed_tools","mode":"auto","tools":[{"type":"function","name":"lookup"},{"type":"custom","name":"sql"}]},"parallel_tool_calls":true,
        "text":{"format":{"type":"json_schema","name":"answer","schema":{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false},"strict":true},"verbosity":"low"},
        "reasoning":{"effort":"low","summary":"auto","context":"all_turns","mode":"standard"},"include":["reasoning.encrypted_content","message.output_text.logprobs"],"top_logprobs":1,"temperature":0.5,"top_p":0.9,"max_output_tokens":128,"truncation":"disabled",
        "metadata":{"suite":"synthetic-local"},"service_tier":"auto","safety_identifier":"synthetic-user","prompt_cache_key":"synthetic-cache","prompt_cache_retention":"in_memory"
    })
}
pub fn output_text(s: &str) -> Value {
    json!({"type":"output_text","text":s,"annotations":[{"type":"url_citation","start_index":0,"end_index":1,"url":"https://example.test/source","title":"Synthetic source"}],"logprobs":[{"token":s,"logprob":-0.25,"bytes":s.as_bytes(),"top_logprobs":[{"token":s,"logprob":-0.25,"bytes":s.as_bytes()}]}]})
}
pub fn response(turn: u8) -> Value {
    let mut request = request(false);
    let o = request.as_object_mut().unwrap();
    for k in ["input", "stream", "include"] {
        o.remove(k);
    }
    o.insert("service_tier".into(), json!("default"));
    o.insert("id".into(), json!(format!("response_{turn}")));
    o.insert("object".into(), json!("response"));
    o.insert("created_at".into(), json!(1.25));
    o.insert("completed_at".into(), json!(2.5));
    o.insert("status".into(), json!("completed"));
    o.insert("error".into(), Value::Null);
    o.insert("incomplete_details".into(), Value::Null);
    o.insert("usage".into(),json!({"input_tokens":3,"output_tokens":5,"total_tokens":8,"input_tokens_details":{"cached_tokens":1,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":2}}));
    o.insert("output".into(),if turn==1{json!([
        {"id":"reasoning","type":"reasoning","status":"completed","summary":[{"type":"summary_text","text":"Plan 🧪"}],"content":[{"type":"reasoning_text","text":"Check"}],"encrypted_content":"synthetic-final-token"},
        {"id":"custom","type":"custom_tool_call","call_id":"c_sql","name":"sql","input":"SELECT 1"},
        {"id":"function","type":"function_call","call_id":"c_lookup","name":"lookup","arguments":"{\"n\":1}","status":"completed"},
        {"id":"message","type":"message","role":"assistant","status":"completed","content":[output_text("{}")]}])}
        else{json!([{ "id":"answer","type":"message","role":"assistant","status":"completed","content":[output_text("{\"ok\":false}")]}])});
    request
}
/// Wire ordering and completion snapshots are from the fixed schema, not from either Rust encoder.
pub fn events(turn: u8) -> Vec<Value> {
    let final_response = response(turn);
    let mut initial = final_response.clone();
    initial["status"] = json!("in_progress");
    initial["output"] = json!([]);
    initial["usage"] = Value::Null;
    initial["completed_at"] = Value::Null;
    let mut events = vec![json!({"type":"response.created","response":initial})];
    for (index, item) in final_response["output"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let id = item["id"].as_str().unwrap();
        let mut added = item.clone();
        match item["type"].as_str().unwrap() {
            "reasoning" => {
                added["status"] = json!("in_progress");
                added["summary"] = json!([]);
                added["content"] = json!([]);
                added["encrypted_content"] = json!("synthetic-partial-token");
            }
            "custom_tool_call" => added["input"] = json!(""),
            "function_call" => {
                added["status"] = json!("in_progress");
                added["arguments"] = json!("");
            }
            "message" => {
                added["status"] = json!("in_progress");
                added["content"] = json!([]);
            }
            _ => unreachable!(),
        }
        events.push(json!({"type":"response.output_item.added","output_index":index,"item":added}));
        match item["type"].as_str().unwrap() {
            "reasoning" => {
                events.push(json!({"type":"response.reasoning_summary_part.added","output_index":index,"item_id":id,"summary_index":0,"part":{"type":"summary_text","text":""}}));
                events.push(json!({"type":"response.reasoning_summary_text.delta","output_index":index,"item_id":id,"summary_index":0,"delta":"Plan 🧪"}));
                events.push(json!({"type":"response.reasoning_summary_text.done","output_index":index,"item_id":id,"summary_index":0,"text":"Plan 🧪"}));
                events.push(json!({"type":"response.reasoning_summary_part.done","output_index":index,"item_id":id,"summary_index":0,"part":{"type":"summary_text","text":"Plan 🧪"}}));
                // Readable reasoning has delta/done, not message content_part events.
                events.push(json!({"type":"response.reasoning_text.delta","output_index":index,"item_id":id,"content_index":0,"delta":"Check"}));
                events.push(json!({"type":"response.reasoning_text.done","output_index":index,"item_id":id,"content_index":0,"text":"Check"}));
            }
            "custom_tool_call" | "function_call" => {
                let custom = item["type"] == "custom_tool_call";
                let (stem, key) = if custom {
                    ("custom_tool_call_input", "input")
                } else {
                    ("function_call_arguments", "arguments")
                };
                events.push(json!({"type":format!("response.{stem}.delta"),"output_index":index,"item_id":id,"delta":item[key]}));
                let mut done = json!({"type":format!("response.{stem}.done"),"output_index":index,"item_id":id});
                done[key] = item[key].clone();
                events.push(done);
            }
            "message" => {
                let part = &item["content"][0];
                let mut probabilities = part["logprobs"].clone();
                for p in probabilities.as_array_mut().unwrap() {
                    p.as_object_mut().unwrap().remove("bytes");
                    for top in p["top_logprobs"].as_array_mut().unwrap() {
                        top.as_object_mut().unwrap().remove("bytes");
                    }
                }
                events.push(json!({"type":"response.content_part.added","output_index":index,"item_id":id,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}));
                events.push(json!({"type":"response.output_text.delta","output_index":index,"item_id":id,"content_index":0,"delta":part["text"],"logprobs":probabilities,"obfuscation":"synthetic-padding"}));
                events.push(json!({"type":"response.output_text.annotation.added","output_index":index,"item_id":id,"content_index":0,"annotation_index":0,"annotation":part["annotations"][0]}));
                events.push(json!({"type":"response.output_text.done","output_index":index,"item_id":id,"content_index":0,"text":part["text"],"logprobs":probabilities}));
                events.push(json!({"type":"response.content_part.done","output_index":index,"item_id":id,"content_index":0,"part":part}));
            }
            _ => unreachable!(),
        }
        events.push(json!({"type":"response.output_item.done","output_index":index,"item":item}));
    }
    events.push(json!({"type":"response.completed","response":final_response}));
    for (i, e) in events.iter_mut().enumerate() {
        e["sequence_number"] = json!(i);
    }
    events
}
