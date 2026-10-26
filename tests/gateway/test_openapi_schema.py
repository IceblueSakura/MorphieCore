"""Independent public OpenAPI structure and operation-specific value expectations."""
import copy
import json
from pathlib import Path
import unittest

from jsonschema import Draft202012Validator, FormatChecker

DOCUMENT = json.loads((Path(__file__).resolve().parents[2] / "docs/openapi.json").read_text())
SCHEMAS = DOCUMENT["components"]["schemas"]


def validator(name):
    return Draft202012Validator({
        "$ref": f"#/components/schemas/{name}",
        "components": {"schemas": SCHEMAS},
    }, format_checker=FormatChecker())


class OpenApiSchemaTests(unittest.TestCase):
    def test_messages_requires_current_version_and_excludes_removed_sampling(self):
        parameters = DOCUMENT["paths"]["/v1/messages"]["post"]["parameters"]
        version = next(p for p in parameters if p.get("name") == "anthropic-version")
        self.assertEqual(version["in"], "header")
        self.assertTrue(version["required"])
        check = Draft202012Validator(version["schema"])
        check.validate("2026-10-21")
        for value in ("", "2023-06-01", "2026-10-22", "2026-10-21,2026-10-21", None):
            self.assertFalse(check.is_valid(value))
        body = {"model": "synthetic", "max_tokens": 32,
                "messages": [{"role": "user", "content": "go"}]}
        request = validator("MessagesRequest")
        request.validate(body)
        request.validate({**body, "unknown_extension": True})
        for field in ("temperature", "top_p", "top_k"):
            for value in (None, 1, 0.5, "invalid"):
                self.assertFalse(request.is_valid({**body, field: value}))

    def test_request_format_and_known_messages_controls_are_nonnull(self):
        chat = {"model": "synthetic", "messages": []}
        responses = {"model": "synthetic", "input": "go"}
        messages = {"model": "synthetic", "max_tokens": 32, "messages": []}
        for schema, body, valid, invalid in (
            ("ChatRequest", chat, {"response_format": {"type": "text"}}, {"response_format": None}),
            ("ResponsesRequest", responses, {"text": {"format": {"type": "text"}}}, {"text": {"format": None}}),
            ("MessagesRequest", messages, {"output_config": {"effort": "low"}}, {"output_config": {"effort": None}}),
            ("MessagesRequest", messages, {"output_config": {"format": {"type": "json_schema"}}}, {"output_config": {"format": None}}),
            ("MessagesRequest", messages, {"tool_choice": {"type": "auto", "disable_parallel_tool_use": False}},
                {"tool_choice": {"type": "auto", "disable_parallel_tool_use": None}}),
        ):
            validator(schema).validate(body)
            validator(schema).validate({**body, **valid})
            self.assertFalse(validator(schema).is_valid({**body, **invalid}))
        self.assertFalse(validator("ResponsesRequest").is_valid({**responses, "text": None}))

    def test_native_thinking_carriers_are_typed_without_an_extension_bag(self):
        chat = {"model": "synthetic", "messages": []}
        response = {"model": "synthetic", "input": "go"}
        for name, body in (("ResponsesRequest", response),):
            for value in (False, True):
                validator(name).validate({**body, "enable_thinking": value})
            for value in (None, 1, "true"):
                self.assertFalse(validator(name).is_valid({**body, "enable_thinking": value}))
            self.assertFalse(validator(name).is_valid({**body, "extra_body": {}}))
            self.assertFalse(validator(name).is_valid({**body, "parameters": {"max_tokens": 64}}))
        for field in ("enable_thinking", "thinking_budget", "top_k"):
            for value in (None, False, True, 32):
                self.assertFalse(validator("ChatRequest").is_valid({**chat, field: value}))
        for value in (False, True):
            validator("ChatRequest").validate({**chat, "preserve_thinking": value})
        for value in (None, 1, "true"):
            self.assertFalse(validator("ChatRequest").is_valid({**chat, "preserve_thinking": value}))
        validator("ChatRequest").validate({**chat, "reasoning_effort": "low"})
        for patch in ({"reasoning_effort": "invalid"}, {"top_k": 0},
                      {"top_k": 101}, {"top_k": 1.5}, {"extra_body": {}},
                      {"parameters": {"max_tokens": 64}}):
            self.assertFalse(validator("ChatRequest").is_valid({**chat, **patch}))

    def test_native_identity_and_history_reading_use_distinct_bounded_carriers(self):
        messages = {"model": "synthetic", "max_tokens": 64,
                    "messages": [{"role": "user", "content": "go"}]}
        for value in (None, "", "用" * 512):
            validator("MessagesRequest").validate({**messages, "metadata": {"user_id": value}})
        for value in ({"metadata": None}, {"metadata": {"user_id": 1}},
                      {"metadata": {"user_id": "x" * 513}}):
            self.assertFalse(validator("MessagesRequest").is_valid({**messages, **value}))

    def test_generation_has_one_neutral_conversation_header_and_no_provider_session_body(self):
        parameter = DOCUMENT["components"]["parameters"]["ConversationId"]
        self.assertEqual(parameter["in"], "header")
        self.assertEqual(parameter["name"], "X-MorphieCore-Conversation-Id")
        self.assertFalse(parameter["required"])
        check = Draft202012Validator(parameter["schema"])
        for value in ("synthetic-conversation", "x" * 256):
            check.validate(value)
        for value in ("", " ", "x,y", "x\ny", "中文", "x" * 257):
            self.assertFalse(check.is_valid(value))
        for path, schema, body in (
            ("/v1/chat/completions", "ChatRequest", {"model": "synthetic", "messages": []}),
            ("/v1/responses", "ResponsesRequest", {"model": "synthetic", "input": "hello"}),
            ("/v1/messages", "MessagesRequest", {
                "model": "synthetic", "max_tokens": 64,
                "messages": [{"role": "user", "content": "hello"}],
            }),
        ):
            validator(schema).validate(body)
            self.assertIn({"$ref": "#/components/parameters/ConversationId"},
                          DOCUMENT["paths"][path]["post"]["parameters"])
            for value in (None, "upstream-session"):
                self.assertFalse(validator(schema).is_valid({**body, "session_id": value}))
        messages = DOCUMENT["paths"]["/v1/messages"]["post"]["responses"]
        self.assertIn("402", messages)
        self.assertIn("529", messages)
        for status in ("401", "405"):
            self.assertEqual(messages[status]["$ref"], "#/components/responses/MessagesError")
        self.assertFalse(validator("MessagesRequest").is_valid({
            "model": "synthetic", "messages": [{"role": "user", "content": "hello"}],
        }))
        chat = {"model": "synthetic", "messages": [], "max_tokens": 8}
        validator("ChatRequest").validate(chat)
        self.assertFalse(validator("ChatRequest").is_valid({**chat, "max_completion_tokens": 8}))
        custom = {"model": "synthetic", "messages": [],
                  "tools": [{"type": "custom", "custom": {"name": "sql", "format": {
                      "type": "grammar", "grammar": {"syntax": "regex", "definition": "SELECT [0-9]+"}}}}],
                  "tool_choice": {"type": "custom", "custom": {"name": "sql"}}}
        validator("ChatRequest").validate(custom)
        malformed = copy.deepcopy(custom)
        malformed["tools"][0]["custom"]["format"]["grammar"]["syntax"] = "unknown"
        self.assertFalse(validator("ChatRequest").is_valid(malformed))

    def test_all_component_schemas_and_local_references_are_valid(self):
        for schema in SCHEMAS.values():
            Draft202012Validator.check_schema(schema)
        def visit(value):
            if isinstance(value, dict):
                if "$ref" in value:
                    self.assertTrue(value["$ref"].startswith("#/"))
                    target = DOCUMENT
                    for part in value["$ref"][2:].split("/"):
                        target = target[part.replace("~1", "/").replace("~0", "~")]
                    self.assertIsInstance(target, dict)
                for child in value.values():
                    visit(child)
            elif isinstance(value, list):
                for child in value:
                    visit(child)
        visit(DOCUMENT)


class EmbeddingSchemaTests(unittest.TestCase):
    def test_text_encoding_request_and_scoped_token_input_shapes(self):
        check = validator("EmbeddingRequest")
        request = {"model": "synthetic-vector", "input": "hello"}
        check.validate(request)
        check.validate({**request, "input": ["first", "第二条"], "dimensions": 2,
                        "encoding_format": "float", "user": "synthetic"})
        check.validate({**request, "encoding_format": "base64"})
        check.validate({**request, "input": [1, 2]})
        check.validate({**request, "input": [[1, 2], [3]]})
        for controls in (
            {"input": ""}, {"input": []}, {"input": ["x", ""]},
            {"input": [-1]}, {"input": [True]}, {"input": [[]]},
            {"input": [4294967296]}, {"input": ["x", 1]}, {"input": None},
            {"input": ["x"] * 129}, {"dimensions": 0}, {"dimensions": True},
            {"dimensions": None}, {"dimensions": 8193},
            {"encoding_format": "binary"}, {"encoding_format": None},
            {"stream": False}, {"provider": None}, {"_openbridge": {}}, {"user": None},
        ):
            self.assertFalse(check.is_valid({**request, **controls}))
        post = DOCUMENT["paths"]["/v1/embeddings"]["post"]
        self.assertEqual(post["requestBody"]["content"]["application/json"]["schema"]["$ref"],
                         "#/components/schemas/EmbeddingRequest")

    def test_complete_embedding_response_requires_usage_counters(self):
        check = validator("EmbeddingResponse")
        response = {"object": "list", "model": "reported-vector",
                    "data": [{"object": "embedding", "index": 0, "embedding": [0.5, -0.125]}],
                    "usage": {"prompt_tokens": 2, "total_tokens": 2}}
        check.validate(response)
        check.validate({**response, "data": [{"object": "embedding", "index": 0,
                                            "embedding": "AAAAPwAAAL4="}]})
        check.validate({**response, "usage": {"prompt_tokens": 0, "total_tokens": 0}})
        for pointer in ("object", "model", "data", "usage"):
            changed = copy.deepcopy(response)
            del changed[pointer]
            self.assertFalse(check.is_valid(changed))
        for data in ([], [{"object": "embedding", "index": -1, "embedding": [1]}],
                     [{"object": "embedding", "index": 0, "embedding": []}],
                     [{"object": "embedding", "index": 0, "embedding": "not-base64"}]):
            self.assertFalse(check.is_valid({**response, "data": data}))
        for usage in (None, 0, {}, {"prompt_tokens": 1}, {"total_tokens": 1},
                      {"prompt_tokens": True, "total_tokens": 1},
                      {"prompt_tokens": -1, "total_tokens": 0}):
            self.assertFalse(check.is_valid({**response, "usage": usage}))


class ImageSchemaTests(unittest.TestCase):
    def test_request_counts_presence_and_control_constraints(self):
        check = validator("ImageGenerationRequest")
        request = {"model": "public-image", "prompt": "synthetic"}
        check.validate(request)
        for n in (None, 1, 2, 10):
            check.validate({**request, "n": n})
        for n in (0, -1, 11, 1.5, True, "2", [], {}):
            self.assertFalse(check.is_valid({**request, "n": n}))
        for controls in (
            {"stream": True}, {"provider": {}}, {"_openbridge": {}},
            {"output_compression": 1}, {"output_format": "png", "output_compression": 0},
            {"output_format": "jpeg", "background": "transparent"},
            {"size": "0x1"}, {"size": "65537x1"}, {"user": None},
        ):
            self.assertFalse(check.is_valid({**request, **controls}))
        check.validate({**request, "n": 2, "output_format": "webp",
                        "background": "transparent", "output_compression": 80})

    def test_response_collection_shape_and_shared_reports(self):
        check = validator("ImageGenerationResponse")
        response = {"created": 7, "data": [{"b64_json": "AQID"}, {"b64_json": "BAUG"}],
                    "output_format": "png", "size": "2x3", "background": "opaque",
                    "quality": "high"}
        check.validate(response)
        for n in (1, 10):
            check.validate({**response, "data": [{"b64_json": "AQID"}] * n})
        for data in ([], None, [{"b64_json": "AQID"}] * 11,
                     [{"url": "https://example.invalid"}],
                     [{"b64_json": "AQID"}, {"b64_json": ""}],
                     [{"b64_json": "AQID", "media_type": "image/png"}]):
            self.assertFalse(check.is_valid({**response, "data": data}))
        changed = copy.deepcopy(response)
        changed.update(output_format="jpeg", background="transparent")
        self.assertFalse(check.is_valid(changed))
        for field in ("output_format", "size", "background", "quality", "usage"):
            check.validate({**response, field: None})
        # Base64 syntax, decoded budgets, count satisfaction and EOF are runtime
        # constraints; contentEncoding alone is not a binary validator.


class ModelsSchemaTests(unittest.TestCase):
    def test_model_required_types_and_unreported_optional_field(self):
        model = {"id": "synthetic-model", "object": "model", "created": 7,
                 "owned_by": "Synthetic Developer"}
        check = validator("Model")
        check.validate(model)
        check.validate({**model, "shutdown_date": None})
        check.validate({**model, "shutdown_date": "2027-01-02"})
        check.validate({**model, "id": "Synthetic-Model"})
        for key in model:
            missing = {k: v for k, v in model.items() if k != key}
            self.assertFalse(check.is_valid(missing))
            self.assertFalse(check.is_valid({**model, key: None}))
        for extra in (
            {"object": "list"}, {"id": "provider/model"}, {"created": True},
            {"created": 0}, {"created": -1}, {"created": 1.5}, {"created": "7"},
            {"owned_by": ""}, {"shutdown_date": "unknown"}, {"price": 1},
            {"credential": "synthetic"}, {"_openbridge": {}},
        ):
            self.assertFalse(check.is_valid({**model, **extra}))

    def test_list_empty_count_and_standard_nonpagination(self):
        check = validator("ModelList")
        model = {"id": "synthetic-model", "object": "model", "created": 7,
                 "owned_by": "Synthetic Developer"}
        check.validate({"object": "list", "data": []})
        check.validate({"object": "list", "data": [model] * 64})
        for value in (
            {"object": "list"}, {"data": []}, {"object": "model", "data": []},
            {"object": "list", "data": None}, {"object": "list", "data": [{}]},
            {"object": "list", "data": [model] * 65},
            {"object": "list", "data": [], "has_more": False},
        ):
            self.assertFalse(check.is_valid(value))

    def test_paths_auth_and_deletion_denial_match_public_contract(self):
        self.assertEqual(DOCUMENT["security"], [{"bearerAuth": []}, {"apiKeyAuth": []}])
        schemes = DOCUMENT["components"]["securitySchemes"]
        self.assertEqual(schemes["apiKeyAuth"]["type"], "apiKey")
        self.assertEqual(schemes["apiKeyAuth"]["in"], "header")
        self.assertEqual(schemes["apiKeyAuth"]["name"], "x-api-key")
        paths = DOCUMENT["paths"]
        self.assertEqual(paths["/v1/models"]["get"]["responses"]["200"]["content"]
                         ["application/json"]["schema"]["$ref"], "#/components/schemas/ModelList")
        single = paths["/v1/models/{model}"]
        self.assertEqual(single["parameters"][0]["name"], "model")
        self.assertTrue(single["parameters"][0]["required"])
        self.assertEqual(single["get"]["responses"]["200"]["content"]
                         ["application/json"]["schema"]["$ref"], "#/components/schemas/Model")
        self.assertNotIn("200", single["delete"]["responses"])
        for method in (paths["/v1/models"]["get"], single["get"], single["delete"]):
            self.assertIn("401", method["responses"])
            self.assertIn("503", method["responses"])
        for status in ("403", "404"):
            self.assertIn(status, single["delete"]["responses"])
        capabilities = paths["/v1/morphiecore/capabilities"]["get"]
        self.assertEqual(capabilities["responses"]["200"]["content"]["application/json"]
                         ["schema"]["$ref"], "#/components/schemas/CapabilitySnapshot")
        self.assertIn("401", capabilities["responses"])
        self.assertIn("503", capabilities["responses"])
        case = {"feature": "response.reported.settings",
                "domain": "incidental_settings_without_safety",
                "treatment": "approved_adaptation",
                "rules": ["chat.omit-incidental-settings.v1"]}
        entry = {"model": "synthetic-model", "protocol": "chat_completions",
                 "delivery_modes": ["json", "sse"], "request": [],
                 "delivery": [case], "history": []}
        snapshot = {"object": "morphiecore.capabilities", "version": 1,
                    "scope": "gateway_contract", "coverage": "partial",
                    "unlisted": "not_assessed", "provider_effectiveness": "not_asserted",
                    "full_loop": "not_asserted", "data": [entry]}
        validator("CapabilitySnapshot").validate(snapshot)
        validator("CapabilitySnapshot").validate({**snapshot, "data": []})
        for changed in ({"full_loop": "supported"}, {"provider_effectiveness": "supported"},
                        {"data": [entry] * 193}, {"credential": "synthetic"}):
            self.assertFalse(validator("CapabilitySnapshot").is_valid({**snapshot, **changed}))
        check = validator("CapabilityCase")
        for changed in ({"rules": []}, {"treatment": "faithful"},
                        {"treatment": "unknown"}, {"body": "synthetic"}):
            self.assertFalse(check.is_valid({**case, **changed}))
        missing = {k: v for k, v in case.items() if k != "rules"}
        self.assertFalse(check.is_valid(missing))
        check.validate({**missing, "treatment": "rejected"})
        for response in ("Generation", "MessagesGeneration"):
            self.assertEqual(DOCUMENT["components"]["responses"][response]["headers"]
                             ["X-MorphieCore-Adaptation"]["$ref"],
                             "#/components/headers/PossibleAdaptation")


class TextModelsSchemaTests(unittest.TestCase):
    def test_capability_states_and_finite_control_choices(self):
        check = validator("TextCapability")
        check.validate({"state": "faithful"})
        check.validate({"state": "approved_adaptation", "rules": ["example.adaptation.v1"]})
        for state in ("unavailable", "non_uniform", "not_assessed"):
            check.validate({"state": state})
            self.assertFalse(check.is_valid({"state": state, "rules": ["example.v1"]}))
        for value in (
            {"state": "approved_adaptation"}, {"state": "approved_adaptation", "rules": []},
            {"state": "approved_adaptation", "rules": ["same", "same"]},
            {"state": "faithful", "rules": ["unused"]}, {"state": "supported"},
        ):
            self.assertFalse(check.is_valid(value))
        effort = validator("TextEffortChoices")
        effort.validate({"state": "faithful", "values": ["low", "high"]})
        effort.validate({"state": "not_assessed"})
        for values in ([], ["none"], ["low", "low"], ["unsupported"]):
            self.assertFalse(effort.is_valid({"state": "faithful", "values": values}))
        self.assertFalse(effort.is_valid({"state": "unavailable", "values": ["low"]}))
        validator("TextActivationChoices").validate({
            "state": "faithful", "values": ["disabled", "adaptive"]})

    def test_text_only_envelopes_bounds_and_no_version_or_pagination(self):
        features = {name: {"state": "not_assessed"} for name in (
            "function_tools", "strict_tool_schema", "parallel_tool_calls",
            "json_object_output", "json_schema_output", "max_output_tokens")}
        limits = {"purpose": "planning", "context_window_tokens": None,
                  "max_input_tokens": None, "max_output_tokens": 65536,
                  "output_token_accounting": "not_assessed"}
        interface = {
            "task": "generation", "protocol": "responses", "delivery_modes": ["json", "sse"],
            "generation": {
                "scope": "route_common", "composition": "not_asserted", "token_limits": limits,
                "features": features, "reasoning": {
                    "activation": {"state": "unavailable"},
                    "effort": {"response": {"state": "unavailable"},
                               "reasoning": {"state": "faithful", "values": ["low", "high"]}},
                },
            },
        }
        model = {"id": "example-model", "name": "example-model",
                 "owned_by": "Example Lab", "interfaces": [interface]}
        envelope = {"object": "morphiecore.model_list", "scope": "generation_text",
                    "coverage": "partial", "unlisted": "not_assessed",
                    "provider_effectiveness": "not_asserted", "full_loop": "not_asserted",
                    "data": [model]}
        check = validator("TextModelList")
        check.validate(envelope)
        check.validate({**envelope, "data": []})
        check.validate({**envelope, "data": [model] * 64})
        validator("TextModelResponse").validate({
            **envelope, "object": "morphiecore.model", "data": model})
        for patch in ({"version": 1}, {"catalog_revision": "rev"},
                      {"has_more": False}, {"data": [model] * 65}):
            self.assertFalse(check.is_valid({**envelope, **patch}))
        for patch in ({"task": "embedding"}, {"protocol": "embeddings"},
                      {"delivery_modes": []}, {"delivery_modes": ["binary"]}):
            self.assertFalse(validator("TextGenerationInterface").is_valid({**interface, **patch}))
        for value in (0, -1, 9007199254740992, "unknown"):
            self.assertFalse(validator("TextTokenLimits").is_valid({
                **limits, "max_output_tokens": value}))
        paths = DOCUMENT["paths"]
        for path, schema in (
            ("/v1/morphiecore/models", "TextModelList"),
            ("/v1/morphiecore/models/{model}", "TextModelResponse"),
        ):
            operation = paths[path]["get"]
            self.assertEqual(operation["responses"]["200"]["content"]["application/json"]
                             ["schema"]["$ref"], f"#/components/schemas/{schema}")
            for status in ("401", "405", "503"):
                self.assertIn(status, operation["responses"])
            self.assertNotIn("delete", paths[path])


class ResponsesSchemaTests(unittest.TestCase):
    def test_default_storage_and_local_snapshot_operations(self):
        request = {"model": "synthetic", "input": "next", "store": True,
                   "previous_response_id": "resp_mc_synthetic"}
        check = validator("ResponsesRequest")
        check.validate(request)
        check.validate({**request, "store": None, "previous_response_id": None})
        for fields in ({"store": "true"}, {"previous_response_id": ""},
                       {"previous_response_id": 7}, {"background": True},
                       {"conversation": "remote-conversation"}):
            self.assertFalse(check.is_valid({**request, **fields}))
        self.assertTrue(SCHEMAS["ResponsesRequest"]["properties"]["store"]["default"])
        stored = {"id": "resp_mc_synthetic", "object": "response", "model": "synthetic",
                  "created_at": 1, "status": "completed", "output": [], "store": True,
                  "previous_response_id": None}
        validator("StoredResponse").validate(stored)
        self.assertFalse(validator("StoredResponse").is_valid({**stored, "store": False}))
        deletion = {"id": "resp_mc_synthetic", "object": "response.deleted", "deleted": True}
        validator("ResponseDeleted").validate(deletion)
        self.assertFalse(validator("ResponseDeleted").is_valid({**deletion, "deleted": False}))
        page = {"object": "list", "data": [], "first_id": None, "last_id": None,
                "has_more": False}
        validator("ResponseInputItems").validate(page)
        self.assertFalse(validator("ResponseInputItems").is_valid({**page, "has_more": "false"}))
        paths = DOCUMENT["paths"]
        for path, method, schema in (
            ("/v1/responses/{response_id}", "get", "StoredResponse"),
            ("/v1/responses/{response_id}", "delete", "ResponseDeleted"),
            ("/v1/responses/{response_id}/input_items", "get", "ResponseInputItems"),
        ):
            operation = paths[path][method]
            self.assertEqual(operation["responses"]["200"]["content"]["application/json"]
                             ["schema"]["$ref"], f"#/components/schemas/{schema}")
            for status in ("400", "401", "404", "429", "503"):
                self.assertIn(status, operation["responses"])

    def test_summary_presence_and_invalid_carriers(self):
        check = validator("ResponsesRequest")
        base = {"model": "synthetic", "input": "hello"}
        check.validate(base)
        for reasoning in (None, {}):
            check.validate({**base, "reasoning": reasoning})
        for value in (None, "synthetic-session", {}):
            self.assertFalse(check.is_valid({**base, "session_id": value}))
        for kind in ("function", "custom"):
            tool = {"type": kind, "name": "lookup"}
            check.validate({**base, "tools": [tool]})
            for field in ("async", "defer_loading"):
                for value in (False, True):
                    check.validate({**base, "tools": [{**tool, field: value}]})
                for value in (None, "false", 0):
                    self.assertFalse(check.is_valid(
                        {**base, "tools": [{**tool, field: value}]}
                    ))
        for field in ("summary", "generate_summary"):
            for value in (None, "auto", "concise", "detailed"):
                check.validate({**base, "reasoning": {field: value}})
            for value in (False, True, 0, "none", [], {}):
                with self.subTest(field=field, value=value):
                    self.assertFalse(
                        check.is_valid({**base, "reasoning": {field: value}})
                    )


class SpeechSchemaTests(unittest.TestCase):
    def test_request_controls_presence_and_deferred_branches(self):
        check = validator("SpeechRequest")
        request = {"model": "public-speech", "input": "你好", "voice": "alloy"}
        check.validate(request)
        for encoding in ("mp3", "opus", "aac", "flac", "wav", "pcm"):
            check.validate({**request, "response_format": encoding, "stream_format": "audio",
                            "instructions": "", "speed": 0.25})
        check.validate({**request, "speed": 4, "input": "你" * 4096})
        for field in ("model", "input", "voice"):
            changed = dict(request)
            del changed[field]
            self.assertFalse(check.is_valid(changed))
        for field in ("input", "voice", "instructions", "speed", "response_format", "stream_format"):
            self.assertFalse(check.is_valid({**request, field: None}))
        for controls in (
            {"voice": {"id": "voice_fixture"}}, {"voice": "alloy\n"},
            {"input": ""}, {"input": "你" * 4097}, {"instructions": "a" * 4097},
            {"stream_format": "sse"}, {"speed": True}, {"speed": "1"},
            {"speed": 0.249}, {"speed": 4.001}, {"response_format": "pcm16"},
            {"stream": False}, {"provider": None}, {"_openbridge": {}},
        ):
            self.assertFalse(check.is_valid({**request, **controls}))

    def test_success_is_binary_not_a_private_json_envelope(self):
        success = DOCUMENT["paths"]["/v1/audio/speech"]["post"]["responses"]["200"]
        self.assertNotIn("application/json", success["content"])
        self.assertNotIn("text/event-stream", success["content"])
        for media in ("application/octet-stream", "audio/mpeg", "audio/wav"):
            self.assertEqual(success["content"][media]["schema"]["format"], "binary")


class TranscriptionSchemaTests(unittest.TestCase):
    def test_single_file_json_branch(self):
        check = validator("TranscriptionRequest")
        request = {"model": "public-asr", "file": "synthetic-file"}
        check.validate(request)
        check.validate({**request, "language": "zh", "response_format": "json", "stream": False})
        for value in (
            {"model": "public-asr"}, {**request, "file": ""},
            {**request, "file": ["one", "two"]}, {**request, "language": None},
            {**request, "response_format": "verbose_json"}, {**request, "stream": True},
            {**request, "prompt": ""}, {**request, "temperature": 0},
            {**request, "provider": {}}, {**request, "file_url": "https://example.invalid"},
        ):
            self.assertFalse(check.is_valid(value))
        post = DOCUMENT["paths"]["/v1/audio/transcriptions"]["post"]
        self.assertEqual(set(post["requestBody"]["content"]), {"multipart/form-data"})

    def test_result_has_text_and_only_reported_duration(self):
        check = validator("TranscriptionResponse")
        check.validate({"text": ""})
        check.validate({"text": "Hi.", "usage": {"type": "duration", "seconds": 1}})
        for value in (
            {}, {"text": None}, {"text": "Hi.", "usage": None},
            {"text": "Hi.", "usage": {"type": "duration", "seconds": -1}},
            {"text": "Hi.", "usage": {"seconds": 1}},
            {"text": "Hi.", "language": "invented"},
            {"text": "Hi.", "_openbridge": {}},
        ):
            self.assertFalse(check.is_valid(value))


if __name__ == "__main__":
    unittest.main()
