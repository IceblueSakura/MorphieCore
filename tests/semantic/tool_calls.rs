//! Tool definitions, calls, results and continuation.
#[path = "tool_calls/continuation.rs"]
mod continuation;
#[path = "tool_calls/function_events.rs"]
mod function_events;
#[path = "tool_calls/structured_events.rs"]
mod structured_events;
#[path = "tool_calls/tool_namespaces.rs"]
mod tool_namespaces;
#[path = "tool_calls/tool_result_errors.rs"]
mod tool_result_errors;
#[path = "tool_calls/tool_results.rs"]
mod tool_results;
#[path = "tool_calls/tools.rs"]
mod tools;
#[path = "tool_calls/turn_continuation.rs"]
mod turn_continuation;

#[path = "tool_calls/creator_dependencies.rs"]
mod creator_dependencies;
#[path = "tool_calls/reported_tool_results.rs"]
mod reported_tool_results;
#[path = "tool_calls/tool_result_creator.rs"]
mod tool_result_creator;
