//! Finite image carrier domains, not a claim about arbitrary media or image decoder limits.
use super::contract::{Capability, Case, assess};
use crate::{gateway::BoundEntry, protocol::ClientProtocol};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
pub(super) struct Input {
    text: Capability,
    image: Vec<ImageDomain>,
}

#[derive(Serialize)]
pub(super) struct Output {
    pub text: Capability,
}

#[derive(Serialize)]
struct ImageDomain {
    placement: &'static str,
    source: &'static str,
    detail: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mime_types: Vec<&'static str>,
    #[serde(flatten)]
    capability: Capability,
}

pub(super) fn input(entry: &BoundEntry, modes: &[&str]) -> Input {
    let mut image = Vec::new();
    for tool_result in [false, true] {
        for inline in [true, false] {
            for auto in [false, true] {
                let formats: &[&str] = if inline {
                    &["image/png", "image/jpeg", "image/webp", "image/gif"]
                } else {
                    // URL resources are inert; discovery never fetches or guesses their MIME.
                    &[""]
                };
                let mut domains: Vec<ImageDomain> = Vec::new();
                for &mime in formats {
                    let case = ImageCase {
                        tool_result,
                        inline,
                        auto,
                        mime,
                    };
                    let capability = assess(entry, modes, Case::Image(case));
                    if !capability.positive() {
                        continue;
                    }
                    // Group only independently assessed formats with identical treatment.
                    if let Some(domain) = domains.iter_mut().find(|d| d.capability == capability) {
                        domain.mime_types.push(mime);
                    } else {
                        domains.push(ImageDomain {
                            placement: if tool_result { "tool_result" } else { "user" },
                            source: if inline { "inline" } else { "url" },
                            detail: if auto { "auto" } else { "omitted" },
                            mime_types: if inline { vec![mime] } else { vec![] },
                            capability,
                        });
                    }
                }
                image.extend(domains);
            }
        }
    }
    Input {
        text: assess(entry, modes, Case::Text),
        image,
    }
}

#[derive(Clone, Copy)]
pub(super) struct ImageCase {
    tool_result: bool,
    inline: bool,
    auto: bool,
    mime: &'static str,
}

impl ImageCase {
    pub(super) fn apply(self, value: &mut Value, protocol: ClientProtocol) -> Option<()> {
        // Synthetic bytes assess the typed carrier, not a Provider's image decoder.
        let data = "AQID";
        let url = if self.inline {
            format!("data:{};base64,{data}", self.mime)
        } else {
            "https://example.invalid/discovery-image".into()
        };
        match protocol {
            ClientProtocol::Chat => {
                if self.tool_result {
                    return None;
                }
                let mut image = json!({"type":"image_url","image_url":{"url":url}});
                if self.auto {
                    image["image_url"]["detail"] = json!("auto");
                }
                value["messages"] = json!([{"role":"user","content":[
                    {"type":"text","text":"Describe this image."}, image
                ]}]);
            }
            ClientProtocol::Responses => {
                let mut image = json!({"type":"input_image","image_url":url});
                if self.auto {
                    image["detail"] = json!("auto");
                }
                value["input"] = if self.tool_result {
                    json!([
                        {"role":"user","content":"Inspect the tool image."},
                        {"type":"function_call","call_id":"image_call","name":"inspect","arguments":"{}"},
                        {"type":"function_call_output","call_id":"image_call","output":[image]}
                    ])
                } else {
                    json!([{"role":"user","content":[
                        {"type":"input_text","text":"Describe this image."}, image
                    ]}])
                };
            }
            ClientProtocol::Messages => {
                // Messages has no image-detail carrier; ignored unknown fields prove nothing.
                if self.auto {
                    return None;
                }
                let source = if self.inline {
                    json!({"type":"base64","media_type":self.mime,"data":data})
                } else {
                    json!({"type":"url","url":url})
                };
                let image = json!({"type":"image","source":source});
                value["messages"] = if self.tool_result {
                    json!([
                        {"role":"user","content":"Inspect the tool image."},
                        {"role":"assistant","content":[
                            {"type":"tool_use","id":"image_call","name":"inspect","input":{}}
                        ]},
                        {"role":"user","content":[
                            {"type":"tool_result","tool_use_id":"image_call","content":[image]}
                        ]}
                    ])
                } else {
                    json!([{"role":"user","content":[
                        {"type":"text","text":"Describe this image."}, image
                    ]}])
                };
            }
        }
        Some(())
    }
}
