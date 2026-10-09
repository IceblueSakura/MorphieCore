"""Independent visual/tool/history expectations and malformed-counterexample gates."""
from copy import deepcopy
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.checks import ProbeFailure
from probe_support.ledger import Run
from probe_support.scenarios import plan_groups


def reasoning():
    return {"type": "reasoning", "id": "r", "content": [
        {"type": "reasoning_text", "text": "synthetic visible reasoning"}]}


def function(key, identity="a", namespace=None):
    import json
    return {"type": "function_call", "id": "fc-" + identity, "call_id": identity,
            "name": "lookup", "namespace": namespace, "arguments": json.dumps({"key": key})}


class CombinationTests(unittest.TestCase):
    def test_call_oracle_requires_independent_keys_unique_identity_and_exact_namespace(self):
        from probe_support.combinations import CallOracle
        oracle = CallOracle(("r5b4", "r3b6"), visible=True)
        calls = [function("r5b4"), function("r3b6", "b")]
        oracle("", calls, [reasoning(), *calls])
        invalid = []
        for field, value in [("call_id", ""), ("call_id", "a"), ("namespace", "other"),
                             ("name", "other"), ("arguments", '{"key":"r3b6","key":"r5b4"}'),
                             ("arguments", '{"key":"r3b6","extra":1}')]:
            changed = deepcopy(calls)
            changed[1][field] = value
            invalid.append(changed)
        invalid.extend([calls[:1], [calls[0], calls[0]], [function("other"), calls[1]]])
        for changed in invalid:
            with self.assertRaises(ProbeFailure):
                oracle("", changed, [reasoning(), *changed])
        for wrong in [[], [{"type": "reasoning", "summary": [
                {"type": "summary_text", "text": "not content"}],
                "encrypted_content": "synthetic"}]]:
            with self.assertRaises(ProbeFailure):
                oracle("", calls, [*wrong, *calls])
        grouped = [function("r5b4", namespace="probe")]
        CallOracle(("r5b4",), namespace="probe")("", grouped, grouped)
        with self.assertRaises(ProbeFailure):
            CallOracle(("r5b4",), namespace="probe")("", [function("r5b4")], [function("r5b4")])

    def test_plan_covers_mixed_rounds_and_rejects_unadmitted_models_protocols(self):
        with tempfile.TemporaryDirectory() as temp:
            for index, bridge in enumerate((False, True)):
                run = Run.create(Path(temp)/str(index), providers="deepseek",
                                 models=["deepseek-flash"], limit=20, tokens=2048,
                                 responses_via_chat=bridge)
                groups = plan_groups(run, run.plan["models"], protocol="responses",
                                     cases=("vision_tool", "vision_parallel",
                                            "vision_parallel_reverse", "vision_replay"))
                self.assertEqual(sum(g[5] for g in groups), 20)
                self.assertEqual(len(groups), 8)
                self.assertTrue(all(g[6] == 2048 for g in groups))
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], protocol="chat", cases=("vision_tool",))
            run = Run.create(Path(temp)/"bad", providers="nvidia",
                             models=["nemotron-3-super"])
            with self.assertRaises(ProbeFailure):
                plan_groups(run, run.plan["models"], protocol="responses", cases=("vision_tool",))

    def test_parallel_noncommutative_answers_and_new_image_replay_are_independent(self):
        from probe_support.combinations import execute
        from probe_support.scenarios import TOOL
        for reverse, expected in [(False, "-43"), (True, "65")]:
            observed = []
            def invoke(n, history, **options):
                observed.append((n, deepcopy(history), options))
                if n == 1:
                    calls = [function("r5b4"), function("r3b6", "b")]
                    options["oracle"]("", calls, [reasoning(), *calls])
                    return [reasoning(), *calls], "", calls
                text = expected if n == 2 else "60"
                items = [reasoning(), {"type":"message", "role":"assistant",
                                      "content":[{"type":"output_text", "text":text}]}]
                options["oracle"](text, [], items)
                return items, text, []
            execute("vision_parallel_reverse" if reverse else "vision_parallel",
                    invoke, False, model="mimo-v2.6-flash", tool=TOOL)
            self.assertEqual([row[2]["streaming"] for row in observed], [False, True, False])
            results = [i for i in observed[1][1] if i.get("type") == "function_call_output"]
            self.assertEqual([i["call_id"] for i in results], ["b", "a"])
            self.assertEqual([i["output"] for i in results], ['{"value": 29}', '{"value": 17}'])
            self.assertEqual(observed[2][1][:len(observed[1][1])], observed[1][1])
            self.assertEqual(observed[2][1][-1]["content"][1]["type"], "input_image")
            self.assertNotEqual(observed[2][1][-1]["content"][1]["image_url"],
                                observed[0][1][0]["content"][1]["image_url"])

    def test_json_oracle_does_not_accept_model_claims_in_place_of_function_calls(self):
        from probe_support.combinations import CallOracle
        with self.assertRaises(ProbeFailure):
            CallOracle(("r5b4:r3b6",))("I called lookup", [], [])

    def test_visual_replay_preserves_explicit_controls_without_adding_tools(self):
        from probe_support.combinations import execute
        from probe_support.scenarios import TOOL
        requested = {"reasoning":{"effort":"minimal"}}
        observed = []
        def invoke(n, history, **options):
            observed.append(deepcopy(options))
            text = "18" if n == 1 else "32"
            output = [reasoning(), {"type":"message","role":"assistant",
                "content":[{"type":"output_text","text":text}]}]
            options["oracle"](text, [], output)
            return output, text, []
        execute("vision_replay", invoke, True, model="mimo-v2.6-pro",
                tool=TOOL, extra=requested)
        self.assertEqual([row["extra"] for row in observed], [requested, requested])
        self.assertEqual(requested, {"reasoning":{"effort":"minimal"}})

    def test_named_function_control_is_explicit_and_keeps_actual_image_history(self):
        from probe_support.combinations import execute, CASES
        from probe_support.scenarios import TOOL
        self.assertEqual(CASES["vision_tool_named"], 2)
        observed = []
        def invoke(n, history, **options):
            observed.append((deepcopy(history), options))
            calls = [function("r5b4:r3b6")] if n == 1 else []
            items = [reasoning(), *calls]
            if n == 2:
                items.append({"type":"message","role":"assistant",
                              "content":[{"type":"output_text","text":"55"}]})
            options["oracle"]("" if n == 1 else "55", calls, items)
            return items, "" if n == 1 else "55", calls
        execute("vision_tool_named", invoke, True, model="mimo-v2.6-flash", tool=TOOL)
        self.assertEqual(observed[0][1]["extra"]["tool_choice"],
                         {"type":"function","name":"lookup"})
        self.assertFalse(observed[0][1]["extra"]["parallel_tool_calls"])
        self.assertEqual(observed[1][1]["extra"]["tool_choice"], "auto")
        self.assertEqual(observed[1][0][0], observed[0][0][0])
        self.assertEqual([row[1]["streaming"] for row in observed], [True, False])

    def test_function_stream_requires_added_deltas_value_done_item_done_and_snapshot(self):
        from probe_support.codecs import response_result
        from test_reasoning_content_probe import Event, snapshot
        item = function("r5b4")
        def events():
            return [
                Event({"type":"response.output_item.added", "output_index":0,
                       "item":{**item, "arguments":""}}),
                Event({"type":"response.function_call_arguments.delta", "output_index":0,
                       "item_id":item["id"], "delta":item["arguments"][:7]}),
                Event({"type":"response.function_call_arguments.delta", "output_index":0,
                       "item_id":item["id"], "delta":item["arguments"][7:]}),
                Event({"type":"response.function_call_arguments.done", "output_index":0,
                       "item_id":item["id"], "arguments":item["arguments"]}),
                Event({"type":"response.output_item.done", "output_index":0, "item":item}),
                Event({"type":"response.completed"}, snapshot([item])),
            ]
        metrics = {}
        result = response_result(events(), True, metrics=metrics, check_function_calls=True)
        self.assertEqual(result[2], [item])
        self.assertTrue(metrics["tool_arguments_stream_ok"])
        for index in (0, 1, 3, 4):
            wrong = events()
            del wrong[index]
            with self.assertRaises(ProbeFailure):
                response_result(wrong, True, check_function_calls=True)
        for index, field, bad in [(1,"item_id","other"), (3,"arguments","{}"),
                                   (4,"output_index",1)]:
            wrong = events()
            wrong[index].value[field] = bad
            with self.assertRaises(ProbeFailure):
                response_result(wrong, True, check_function_calls=True)
        wrong = events()
        wrong[-1].response = snapshot([{**item, "call_id":"foreign"}])
        with self.assertRaises(ProbeFailure):
            response_result(wrong, True, check_function_calls=True)

    def test_grid_counts_are_read_from_png_pixels_not_generator_counts(self):
        import base64, struct, zlib
        from probe_support.images import _grid_png
        for grid, expected in [(("RRB","BRR","BBR"), (5,4)),
                               (("BRB","BBR","RBB"), (3,6)),
                               (("RRB","BBB","BBB"), (2,7))]:
            data = base64.b64decode(_grid_png(grid).split(",",1)[1])
            position, compressed = 8, b""
            while position < len(data):
                size = struct.unpack(">I", data[position:position+4])[0]
                kind = data[position+4:position+8]
                payload = data[position+8:position+8+size]
                if kind == b"IDAT":
                    compressed += payload
                position += size+12
            rows = zlib.decompress(compressed)
            centers = [rows[y*577+1+x*3:y*577+4+x*3]
                       for y in (32,96,160) for x in (32,96,160)]
            self.assertEqual((centers.count(b"\xff\x00\x00"),
                              centers.count(b"\x00\x00\xff")), expected)
        self.assertEqual(17*6-29*5, -43)
        self.assertEqual(29*4-17*3, 65)
        self.assertEqual(17+29+2*7, 60)

    def test_uncapped_plan_is_explicit_isolated_and_never_accepts_a_token_cap(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp)/"siwc", providers="openai-siwc",
                             models=["gpt-6.1-sol"], limit=16, tokens=None,
                             uncapped_siwc=True)
            from unittest.mock import patch
            with patch("probe_support.ledger.time.time",
                       return_value=run.plan["created"] + 2161):
                with self.assertRaises(ProbeFailure):
                    run.reserve("gpt-6.1-sol", "expired", None)
            run = Run(run.directory)
            self.assertEqual(run.plan["version"], 3)
            self.assertTrue(run.is_uncapped)
            self.assertTrue(run.valid_budget("gpt-6.1-sol", None))
            for cap in (0, 1, 2048, True):
                self.assertFalse(run.valid_budget("gpt-6.1-sol", cap))
            groups = plan_groups(run, run.plan["models"], protocol="responses",
                                cases=("text_tool", "text_parallel", "text_parallel_reverse"))
            self.assertEqual(sum(g[5] for g in groups), 16)
            self.assertTrue(all(g[6] is None for g in groups))
            baseline = plan_groups(run, run.plan["models"], protocol="responses", cases=("text",))
            self.assertEqual(sum(g[5] for g in baseline), 2)
            self.assertTrue(all(g[6] is None for g in baseline))
            for case in ("vision_tool", "schema", "length", "tool"):
                with self.assertRaises(ProbeFailure):
                    plan_groups(run, run.plan["models"], protocol="responses", cases=(case,))
            for index, kwargs in enumerate([
                {"tokens":2048}, {"responses_via_chat":True},
                {"models":["grok-4.7"], "providers":"grok"}, {"limit":33},
            ]):
                options = dict(providers="openai-siwc", models=["gpt-6.1-sol"],
                               tokens=None, uncapped_siwc=True)
                options.update(kwargs)
                with self.assertRaises(ProbeFailure):
                    Run.create(Path(temp)/f"bad{index}", **options)

    def test_uncapped_send_rejects_even_null_caps_before_reservation_or_network(self):
        import httpx2
        from probe_support.runtime import ProbeClient
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp)/"siwc", providers="openai-siwc",
                             models=["gpt-6.1-sol"], limit=16, tokens=None,
                             uncapped_siwc=True)
            with ProbeClient("http://127.0.0.1:1", run) as transport:
                for field in ("max_output_tokens", "max_completion_tokens", "max_tokens"):
                    for value in (None, 8):
                        transport.prepare("gpt-6.1-sol", "responses", "synthetic", [])
                        request = httpx2.Request("POST", transport.origin+"/v1/responses",
                            json={"model":"gpt-6.1-sol", "input":[], field:value})
                        with patch("openai.DefaultHttpxClient.send",
                                   side_effect=AssertionError("no network")):
                            with self.assertRaises(ProbeFailure):
                                transport.send(request)
                self.assertEqual(run.snapshot(), [])

    def test_responses_probe_requests_unpadded_frames_without_relaxing_wire_budget(self):
        from types import SimpleNamespace
        from unittest.mock import Mock
        from probe_support.scenarios import call
        for streaming in (False, True):
            send = Mock(side_effect=RuntimeError("synthetic stop before I/O"))
            client = SimpleNamespace(responses=SimpleNamespace(create=send))
            transport = SimpleNamespace(prepare=Mock(), wire=None, last_status=None, attempt=None)
            with self.assertRaises(ProbeFailure):
                call(client, transport, "gpt-6.1-sol", "responses", "synthetic", [],
                     streaming, cap=None)
            params = send.call_args.kwargs
            self.assertNotIn("max_output_tokens", params)
            if streaming:
                self.assertEqual(params["stream_options"], {"include_obfuscation":False})
            else:
                self.assertNotIn("stream_options", params)

    def test_closed_static_incomplete_response_is_an_oracle_terminal_not_wire_failure(self):
        from types import SimpleNamespace
        from unittest.mock import Mock
        from probe_support.scenarios import call
        metrics = []
        result = SimpleNamespace(status="incomplete", usage=None, output=[])
        transport = SimpleNamespace(
            prepare=Mock(), wire=SimpleNamespace(closed=True, terminal=None, bytes=99),
            last_status=200, attempt="synthetic")
        transport.complete = lambda state, values: metrics.append((state, deepcopy(values))) or "synthetic"
        client = SimpleNamespace(responses=SimpleNamespace(create=lambda **_: result))
        with self.assertRaises(ProbeFailure):
            call(client, transport, "mimo-v2.6-pro", "responses", "synthetic", [], False)
        self.assertEqual(metrics[0][0], "oracle_failed")
        self.assertEqual(metrics[0][1]["terminal"], "response.incomplete")
        self.assertTrue(metrics[0][1]["sdk_consumed"])
