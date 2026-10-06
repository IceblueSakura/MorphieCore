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
        self.assertEqual(DOCUMENT["security"], [{"bearerAuth": []}])
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


class ResponsesSchemaTests(unittest.TestCase):
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
