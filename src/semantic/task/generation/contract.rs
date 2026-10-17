//! Task admission vocabulary. No wire profiles, routing, credentials or cache policy.
use crate::semantic::task::generation::{
    GenerationRequest, GenerationRequirements, GenerationResponse, GenerationResponseRequirements,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum GenerationFeature {
    Instructions,
    OrderedInstructions,
    Temperature,
    MaxOutputTokens,
    Tools,
    ProviderObservation,
    TextMetadata,
    Controls,
    ParallelTools,
    StrictTools,
    ToolSchemaNormalization,
    ExplicitCache,
    CachedContext,
    Safety,
    AutomaticPrefixCache,
    StructuredArguments,
    StructuredToolResult,
    ToolExecutionReport,
    ToolResultError,
    ToolResultMedia,
    StructuredOutput,
    Reasoning,
    ResponseEffort,
    Compaction,
    ImageInput,
    AudioInput,
    VideoInput,
    AudioOutput,
    AudioHistory,
    FileInput,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerationSemanticContract {
    pub instructions: bool,
    /// Instruction items after conversation content keep their position.
    pub ordered_instructions: bool,
    pub temperature: bool,
    pub max_output_tokens: bool,
    pub tools: bool,
    pub custom_tools: bool,
    pub structured_arguments: bool,
    pub structured_tool_results: bool,
    pub tool_execution_reports: bool,
    pub tool_result_errors: bool,
    pub tool_result_media: bool,
    pub text_metadata: bool,
    pub top_p: bool,
    pub top_k: bool,
    pub stop_sequences: bool,
    pub presence_penalty: bool,
    pub frequency_penalty: bool,
    pub seed: bool,
    pub logprobs: bool,
    pub verbosity: bool,
    pub truncation: bool,
    pub structured_output: bool,
    pub reasoning: bool,
    pub response_effort: bool,
    pub compaction: bool,
    pub image_input: bool,
    pub audio_input: bool,
    pub video_input: bool,
    pub audio_output: bool,
    pub audio_history: bool,
    pub file_input: bool,
    pub parallel_tool_calls: bool,
    pub strict_tools: bool,
    /// Omitted strictness that requests schema normalization, not an explicit strict tool.
    pub normalization_default_tools: bool,
    pub explicit_cache: bool,
    pub cached_context: bool,
    pub safety: bool,
    pub automatic_prefix_cache: bool,
}
impl GenerationSemanticContract {
    /// Complete vocabulary, useful for codecs and synthetic fixtures, not model discovery.
    pub fn full() -> Self {
        Self {
            response_effort: true,
            compaction: true,
            instructions: true,
            ordered_instructions: true,
            temperature: true,
            max_output_tokens: true,
            tools: true,
            custom_tools: true,
            structured_arguments: true,
            structured_tool_results: true,
            tool_execution_reports: true,
            tool_result_errors: true,
            tool_result_media: true,
            text_metadata: true,
            top_p: true,
            top_k: true,
            stop_sequences: true,
            presence_penalty: true,
            frequency_penalty: true,
            seed: true,
            logprobs: true,
            verbosity: true,
            truncation: true,
            structured_output: true,
            reasoning: true,
            image_input: true,
            audio_input: true,
            video_input: true,
            audio_output: true,
            audio_history: true,
            file_input: true,
            parallel_tool_calls: true,
            strict_tools: true,
            normalization_default_tools: true,
            explicit_cache: true,
            cached_context: true,
            safety: true,
            automatic_prefix_cache: true,
        }
    }
    /// Explicit currently admitted text/image slice; new fields require a decision here.
    pub fn text_images() -> Self {
        Self {
            response_effort: false,
            compaction: false,
            instructions: true,
            ordered_instructions: true,
            temperature: true,
            max_output_tokens: true,
            tools: true,
            custom_tools: false,
            structured_arguments: false,
            structured_tool_results: false,
            tool_execution_reports: false,
            tool_result_errors: false,
            tool_result_media: false,
            text_metadata: true,
            top_p: true,
            top_k: false,
            stop_sequences: false,
            presence_penalty: false,
            frequency_penalty: false,
            seed: false,
            logprobs: true,
            verbosity: true,
            truncation: true,
            structured_output: true,
            reasoning: true,
            image_input: true,
            audio_input: false,
            video_input: false,
            audio_output: false,
            audio_history: false,
            file_input: false,
            parallel_tool_calls: true,
            strict_tools: false,
            // Preserve the admitted default separately from explicit strict-tool support.
            normalization_default_tools: true,
            explicit_cache: false,
            cached_context: false,
            safety: false,
            automatic_prefix_cache: false,
        }
    }
    pub fn supports(&self, promise: &Self) -> bool {
        [
            (promise.instructions, self.instructions),
            (promise.ordered_instructions, self.ordered_instructions),
            (promise.temperature, self.temperature),
            (promise.max_output_tokens, self.max_output_tokens),
            (promise.tools, self.tools),
            (promise.custom_tools, self.custom_tools),
            (promise.structured_arguments, self.structured_arguments),
            (
                promise.structured_tool_results,
                self.structured_tool_results,
            ),
            (promise.tool_execution_reports, self.tool_execution_reports),
            (promise.tool_result_errors, self.tool_result_errors),
            (promise.tool_result_media, self.tool_result_media),
            (promise.text_metadata, self.text_metadata),
            (promise.top_p, self.top_p),
            (promise.top_k, self.top_k),
            (promise.stop_sequences, self.stop_sequences),
            (promise.presence_penalty, self.presence_penalty),
            (promise.frequency_penalty, self.frequency_penalty),
            (promise.seed, self.seed),
            (promise.logprobs, self.logprobs),
            (promise.verbosity, self.verbosity),
            (promise.truncation, self.truncation),
            (promise.structured_output, self.structured_output),
            (promise.reasoning, self.reasoning),
            (promise.response_effort, self.response_effort),
            (promise.compaction, self.compaction),
            (promise.image_input, self.image_input),
            (promise.audio_input, self.audio_input),
            (promise.video_input, self.video_input),
            (promise.audio_output, self.audio_output),
            (promise.audio_history, self.audio_history),
            (promise.file_input, self.file_input),
            (promise.parallel_tool_calls, self.parallel_tool_calls),
            (promise.strict_tools, self.strict_tools),
            (
                promise.normalization_default_tools,
                self.normalization_default_tools,
            ),
            (promise.explicit_cache, self.explicit_cache),
            (promise.cached_context, self.cached_context),
            (promise.safety, self.safety),
            (promise.automatic_prefix_cache, self.automatic_prefix_cache),
        ]
        .into_iter()
        .all(|(required, supported)| !required || supported)
    }
    /// Actual output facts have their own requirements, not a fabricated request.
    pub fn check_response(
        &self,
        response: &GenerationResponse,
    ) -> Result<GenerationResponseRequirements, GenerationFeature> {
        use GenerationFeature::*;
        let q = GenerationResponseRequirements::derive(response);
        if q.provider_observations {
            return Err(ProviderObservation);
        }
        for (needed, supported, feature) in [
            (q.safety_reports, self.safety, Safety),
            (q.compaction, self.compaction, Compaction),
            (q.audio_output, self.audio_output, AudioOutput),
            (q.instructions, self.instructions, Instructions),
            (q.tools, self.tools, Tools),
            (q.custom_tools, self.custom_tools, Tools),
            (
                q.structured_arguments,
                self.structured_arguments,
                StructuredArguments,
            ),
            (
                q.structured_tool_results,
                self.structured_tool_results,
                StructuredToolResult,
            ),
            (
                q.tool_execution_reports,
                self.tool_execution_reports,
                ToolExecutionReport,
            ),
            (
                q.tool_result_errors,
                self.tool_result_errors,
                ToolResultError,
            ),
            (q.tool_result_media, self.tool_result_media, ToolResultMedia),
            (q.reasoning, self.reasoning, Reasoning),
            (q.response_effort, self.response_effort, ResponseEffort),
            (q.text_metadata, self.text_metadata, TextMetadata),
            (q.logprobs, self.logprobs, Controls),
        ] {
            if needed && !supported {
                return Err(feature);
            }
        }
        Ok(q)
    }
    /// Validation remains owned by GenerationRequest; this checks semantic support only.
    pub fn check(
        &self,
        r: &GenerationRequest,
    ) -> Result<GenerationRequirements, GenerationFeature> {
        use GenerationFeature::*;
        let q = GenerationRequirements::derive(r);
        if q.provider_observations {
            return Err(ProviderObservation);
        }
        for (required, supported, feature) in [
            (q.safety_settings, self.safety, Safety),
            (q.compaction, self.compaction, Compaction),
            (q.instruction_count > 0, self.instructions, Instructions),
            (
                q.ordered_instructions,
                self.ordered_instructions,
                OrderedInstructions,
            ),
            (q.temperature, self.temperature, Temperature),
            (
                q.max_output_tokens.is_some(),
                self.max_output_tokens,
                MaxOutputTokens,
            ),
            (
                q.tool_count > 0
                    || q.tool_history
                    || q.tool_choice.is_some()
                    || q.parallel_tool_calls.is_some(),
                self.tools,
                Tools,
            ),
            (q.custom_tools, self.custom_tools, Tools),
            (
                q.structured_arguments,
                self.structured_arguments,
                StructuredArguments,
            ),
            (
                q.structured_tool_results,
                self.structured_tool_results,
                StructuredToolResult,
            ),
            (
                q.tool_execution_reports,
                self.tool_execution_reports,
                ToolExecutionReport,
            ),
            (
                q.tool_result_errors,
                self.tool_result_errors,
                ToolResultError,
            ),
            (
                q.tool_result_media > 0,
                self.tool_result_media,
                ToolResultMedia,
            ),
            (q.text_metadata, self.text_metadata, TextMetadata),
            (q.top_p, self.top_p, Controls),
            (q.top_k, self.top_k, Controls),
            (q.stop_sequences, self.stop_sequences, Controls),
            (q.presence_penalty, self.presence_penalty, Controls),
            (q.frequency_penalty, self.frequency_penalty, Controls),
            (q.seed, self.seed, Controls),
            (q.logprobs, self.logprobs, Controls),
            (q.truncation, self.truncation, Controls),
            (q.verbosity, self.verbosity, Controls),
            (
                q.parallel_tool_calls == Some(true),
                self.parallel_tool_calls,
                ParallelTools,
            ),
            (q.strict_function_tools, self.strict_tools, StrictTools),
            (
                q.normalization_default_tools,
                self.normalization_default_tools,
                ToolSchemaNormalization,
            ),
            (q.explicit_cache, self.explicit_cache, ExplicitCache),
            (q.cached_context, self.cached_context, CachedContext),
            (
                q.automatic_prefix_cache,
                self.automatic_prefix_cache,
                AutomaticPrefixCache,
            ),
            (
                q.structured_output,
                self.structured_output,
                StructuredOutput,
            ),
            (q.reasoning, self.reasoning, Reasoning),
            (q.response_effort, self.response_effort, ResponseEffort),
            (q.image_inputs > 0, self.image_input, ImageInput),
            (q.audio_inputs > 0, self.audio_input, AudioInput),
            (q.video_inputs > 0, self.video_input, VideoInput),
            (q.audio_output, self.audio_output, AudioOutput),
            (q.audio_history, self.audio_history, AudioHistory),
            (q.file_inputs > 0, self.file_input, FileInput),
        ] {
            if required && !supported {
                return Err(feature);
            }
        }
        Ok(q)
    }
}
