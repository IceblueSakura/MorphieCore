"""Independent offline boundary fixtures; never load workspace credentials."""

import concurrent.futures
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples"))
from probe_support.ledger import Run
from probe_support.checks import ProbeFailure
from probe_support.wire import Wire


def reserve_worker(path, number):
    try:
        return Run(path).reserve("nemotron-3-super", f"case-{number}", 8)
    except ProbeFailure:
        return None


class ProbeCoreTests(unittest.TestCase):
    def test_source_fingerprint_includes_typescript_and_toolchain(self):
        from probe_support.ledger import source_fingerprint
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = ("Cargo.toml", "Cargo.lock", "tests/sdk/uv.lock",
                     "package.json", "package-lock.json", "tsconfig.json",
                     "examples/synthetic.ts", "tests/sdk/pyproject.toml",
                     "tests/sdk/.python-version", "rust-toolchain.toml")
            for name in paths:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("{}")
            for name in paths[3:]:
                before = source_fingerprint(root)
                path = root / name
                path.write_text(path.read_text() + "\n")
                self.assertNotEqual(source_fingerprint(root), before, name)

    def test_model_subset_is_enforced_by_persistent_reservations(self):
        from probe_support.scenarios import plan_groups

        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(
                Path(temp) / "run",
                providers="xiaomi,longcat",
                models=["mimo-v2.6-flash", "longcat-2.5-preview"],
                limit=72,
            )
            run = Run(run.directory)
            with self.assertRaises(ProbeFailure):
                run.reserve("mimo-v2.6-pro", "unselected", 8)
            groups = plan_groups(
                run, run.plan["models"],
                cases=("text", "json", "tool", "history", "length", "cancel"),
            )
            self.assertEqual(sum(group[5] for group in groups), 72)
            run.reserve("mimo-v2.6-flash", "selected", 8)

    def test_go_matrix_is_six_requests_with_stable_tool_conversation_sessions(self):
        from contextlib import nullcontext
        from probe_support.scenarios import matrix, plan_groups
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="opencode-go",
                models=["hy4-preview"], limit=6, tokens=2048)
            groups = plan_groups(run, run.plan["models"], cases=("text", "tool"))
            self.assertEqual(sum(group[5] for group in groups), 6)
            self.assertTrue(all(group[1] == "chat" and group[6] == 2048 for group in groups))
            with patch("probe_support.scenarios.session", return_value=nullcontext((None, None))), patch(
                "probe_support.scenarios.call", return_value=([], "", [{"id":"synthetic-call"}])
            ) as send:
                self.assertTrue(matrix(run, run.plan["models"], cases=("text", "tool")))
            sessions = [call.kwargs["extra"]["extra_body"]["session_id"] for call in send.call_args_list]
            self.assertEqual(len(sessions), 6)
            self.assertEqual(sessions[1], sessions[2])
            self.assertEqual(sessions[4], sessions[5])
            self.assertEqual(len(set(sessions)), 4)
            self.assertTrue(all(value.isascii() and len(value) <= 256 for value in sessions))

    def test_pdf_matrix_is_four_requests_and_preserves_actual_history(self):
        from contextlib import nullcontext
        from probe_support.scenarios import matrix, plan_groups
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="openrouter",
                models=["gpt-6-luna"], limit=4, tokens=512)
            groups = plan_groups(run, run.plan["models"], cases=("file",), protocol="responses")
            self.assertEqual(sum(group[5] for group in groups), 4)
            self.assertTrue(all(g[1] == "responses" and g[6] == 512 for g in groups))
            with patch("probe_support.scenarios.session", return_value=nullcontext((None, None))), patch(
                "probe_support.scenarios.call", return_value=([
                    {"type":"message","role":"assistant","content":[{"type":"output_text","text":"synthetic"}]}
                ], "", [])
            ) as send:
                self.assertTrue(matrix(run, run.plan["models"], cases=("file",), protocol="responses"))
            self.assertEqual(send.call_count, 4)
            second = send.call_args_list[1].args[5]
            self.assertEqual(len(second), 3)
            self.assertEqual(second[0]["content"][1]["type"], "input_file")
            self.assertTrue(all(c.kwargs["cap"] == 512 for c in send.call_args_list))
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="opencode-go",
                models=["hy4-preview"], limit=4, tokens=512)
            with self.assertRaises(ProbeFailure):
                plan_groups(run, run.plan["models"], cases=("file",))

    def test_openrouter_file_replay_uses_two_deliveries_and_actual_output(self):
        from contextlib import nullcontext
        from probe_support.scenarios import matrix, plan_groups
        output = [{"type":"reasoning","id":"synthetic","summary":[],
                   "encrypted_content":"synthetic-opaque"}]
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="openrouter",
                models=["gpt-6-luna"], limit=2, tokens=512)
            groups = plan_groups(run, run.plan["models"], cases=("file_replay",), protocol="responses")
            self.assertEqual(len(groups), 1)
            self.assertEqual(groups[0][5:], (2,512))
            with patch("probe_support.scenarios.session", return_value=nullcontext((None,None))), patch(
                "probe_support.scenarios.call", return_value=(output,"",[])
            ) as send:
                self.assertTrue(matrix(run, run.plan["models"], cases=("file_replay",), protocol="responses"))
            self.assertEqual(send.call_count, 2)
            self.assertTrue(send.call_args_list[0].args[6])
            self.assertFalse(send.call_args_list[1].args[6])
            history = send.call_args_list[1].args[5]
            self.assertEqual(history[1], output[0])
            self.assertEqual(history[0]["content"][1]["type"], "input_file")
            self.assertTrue(all(c.kwargs["cap"] == 512 for c in send.call_args_list))
            with self.assertRaises(ProbeFailure):
                plan_groups(run,run.plan["models"],cases=("file_replay",),protocol="chat")
            with patch("probe_support.scenarios.session", return_value=nullcontext((None,None))), patch(
                "probe_support.scenarios.call", side_effect=ProbeFailure("synthetic","transport")
            ) as send:
                self.assertFalse(matrix(run,run.plan["models"],cases=("file_replay",),protocol="responses"))
            self.assertEqual(send.call_count,1)

    def test_file_reasoning_requires_opaque_and_preserves_three_turn_history(self):
        from contextlib import nullcontext
        from copy import deepcopy
        from probe_support.scenarios import matrix, plan_groups
        output = [{"type":"reasoning","id":"synthetic","summary":[],
                   "encrypted_content":"synthetic-cipher"}]
        observed = []
        def send(*args, **kwargs):
            observed.append((deepcopy(args[5]),args[6],deepcopy(kwargs)))
            kwargs["oracle"](["BUILD-A17","PATCH-B29","BUILD-A17"][len(observed)-1],[],output)
            return deepcopy(output),"",[]
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp)/"run", providers="openrouter",
                models=["gpt-6-luna"], limit=3, tokens=1024)
            groups=plan_groups(run,run.plan["models"],cases=("file_reasoning",),protocol="responses")
            self.assertEqual(len(groups),1)
            self.assertEqual(groups[0][5:],(3,1024))
            with patch("probe_support.scenarios.session",return_value=nullcontext((None,None))), patch(
                "probe_support.scenarios.call",side_effect=send):
                self.assertTrue(matrix(run,run.plan["models"],cases=("file_reasoning",),protocol="responses"))
            self.assertEqual([r[1] for r in observed],[True,False,True])
            self.assertEqual(observed[1][0][1],output[0])
            self.assertEqual(observed[2][0][3],output[0])
            for history,stream,kw in observed:
                self.assertEqual(history[0]["content"][1]["type"],"input_file")
                self.assertEqual(kw["extra"]["include"],["reasoning.encrypted_content"])
                self.assertEqual(kw["extra"]["reasoning"]["effort"],"medium")
            with self.assertRaises(ProbeFailure):
                observed[0][2]["oracle"]("BUILD-A17",[],[])
            with patch("probe_support.scenarios.session",return_value=nullcontext((None,None))), patch(
                "probe_support.scenarios.call",side_effect=ProbeFailure("synthetic","transport")) as fail:
                self.assertFalse(matrix(run,run.plan["models"],cases=("file_reasoning",),protocol="responses"))
                self.assertEqual(fail.call_count,1)

    def test_file_reasoning_math_uses_file_factors_and_exact_oracles(self):
        from contextlib import nullcontext
        from probe_support.scenarios import matrix
        output=[{"type":"reasoning","id":"synthetic","encrypted_content":"synthetic"}]
        count=0
        def send(*args,**kw):
            nonlocal count
            expected=(17*29,17*29+17,17*29+17+29)[count]
            count+=1
            check=kw["oracle"]
            check(json.dumps({"answer":expected}),[],output)
            for bad in (json.dumps({"answer":expected+1}),json.dumps({"answer":str(expected)})):
                with self.assertRaises(ProbeFailure): check(bad,[],output)
            with self.assertRaises(ProbeFailure): check(json.dumps({"answer":expected}),[],[])
            self.assertEqual(args[6],count!=2)
            self.assertEqual(kw["cap"],1024)
            return output,"",[]
        with tempfile.TemporaryDirectory() as temp:
            run=Run.create(Path(temp)/"run",providers="openrouter",models=["gpt-6-luna"],limit=3,tokens=1024)
            with patch("probe_support.scenarios.session",return_value=nullcontext((None,None))),patch(
                "probe_support.scenarios.call",side_effect=send):
                self.assertTrue(matrix(run,run.plan["models"],cases=("file_reasoning_math",),protocol="responses"))
        self.assertEqual(count,3)

    def test_file_url_matrix_has_four_bounded_requests_and_preserves_source(self):
        from contextlib import nullcontext
        from copy import deepcopy
        from probe_support.scenarios import matrix, plan_groups
        captured=[]
        def send(*args,**kwargs):
            captured.append(deepcopy(args[5]))
            kwargs["oracle"]("Dummy PDF file" if len(captured)%2 else "3",[],[])
            return [{"type":"message","role":"assistant","content":[
                {"type":"output_text","text":"synthetic"}]}],"",[]
        with tempfile.TemporaryDirectory() as temp:
            run=Run.create(Path(temp)/"run",providers="openrouter",models=["gpt-6-luna"],limit=4,tokens=512)
            groups=plan_groups(run,run.plan["models"],cases=("file_url",),protocol="responses")
            self.assertEqual(sum(g[5] for g in groups),4)
            self.assertTrue(all(g[6]==512 for g in groups))
            with patch("probe_support.scenarios.session",return_value=nullcontext((None,None))),patch(
                "probe_support.scenarios.call",side_effect=send):
                self.assertTrue(matrix(run,run.plan["models"],cases=("file_url",),protocol="responses"))
        for i in (0,2):
            self.assertEqual(captured[i][0],captured[i+1][0])
            part=captured[i][0]["content"][1]
            self.assertEqual(set(part),{"type","file_url"})
            self.assertEqual(part["file_url"],"https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf")
            self.assertEqual(len(captured[i+1]),3)

    def test_client_owned_file_continuation_is_one_bounded_diagnostic_request(self):
        from probe_support.files import file_continuation_history
        from probe_support.scenarios import plan_groups
        from probe_support.ledger import closed_metrics
        history = file_continuation_history()
        self.assertEqual(len(history), 3)
        self.assertEqual(history[1]["content"][0]["logprobs"], [])
        self.assertNotIn("PATCH-B29", history[1]["content"][0]["text"] + history[2]["content"])
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="openrouter",
                models=["gpt-6-luna"], limit=1, tokens=512)
            groups = plan_groups(run, run.plan["models"], cases=("file_continue",), delivery="sse", protocol="responses")
            self.assertEqual(sum(g[5] for g in groups), 1)
        self.assertEqual(closed_metrics({"decode_failure":"invalid_item_snapshot", "event_items":2}),
            {"decode_failure":"invalid_item_snapshot", "event_items":2})
        with self.assertRaises(RuntimeError):
            closed_metrics({"decode_failure":"synthetic-private-body"})

    def test_pdf_fixture_has_independent_offsets_lengths_and_document_markers(self):
        import base64
        import re
        from probe_support.files import file_history
        history = file_history()
        content = history[0]["content"]
        self.assertEqual([p["type"] for p in content], ["input_text","input_file","input_text"])
        raw = base64.b64decode(content[1]["file_data"].split(",", 1)[1], validate=True)
        self.assertLess(len(raw), 16384)
        self.assertTrue(raw.startswith(b"%PDF-1.4\n"))
        self.assertIn(b"BUILD-A17", raw)
        self.assertIn(b"PATCH-B29", raw)
        self.assertNotIn("BUILD-A17", content[0]["text"] + content[2]["text"])
        xref = int(re.search(rb"startxref\n([0-9]+)\n%%EOF", raw).group(1))
        self.assertEqual(raw[xref:xref+4], b"xref")
        rows = raw[xref:].splitlines()
        self.assertEqual(rows[1], b"0 6")
        for number, row in enumerate(rows[3:8], 1):
            offset = int(row.split()[0])
            self.assertTrue(raw[offset:].startswith(f"{number} 0 obj\n".encode()))
        self.assertIn(b"/Count 1", raw)
        stream = re.search(rb"/Length ([0-9]+) >>\nstream\n(.*?)endstream", raw, re.S)
        self.assertEqual(len(stream.group(2)), int(stream.group(1)))
        from probe_support.scenarios import expect_file_marker
        expect_file_marker("BUILD-A17")("BUILD-A17", [], [])
        with self.assertRaises(ProbeFailure):
            expect_file_marker("BUILD-A17")("PATCH-B29", [], [])
        with self.assertRaises(ProbeFailure):
            expect_file_marker("BUILD-A17")("BUILD-A17", [{"name":"unexpected"}], [])

    def test_image_matrix_is_eight_bounded_requests_with_independent_pixels(self):
        import base64
        import struct
        import zlib
        from probe_support.scenarios import plan_groups
        from probe_support.images import image_history

        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", providers="deepseek,xiaomi",
                models=["deepseek-flash", "mimo-v2.6-flash"], limit=8, tokens=512)
            groups = plan_groups(run, run.plan["models"], cases=("image",), effort="none")
            self.assertEqual(len(groups), 8)
            self.assertEqual(sum(group[5] for group in groups), 8)
            self.assertTrue(all(group[6] == 512 for group in groups))
        for protocol in ("chat", "responses"):
            content = image_history(protocol)[0]["content"]
            self.assertEqual(len(content), 4)
            for index, rgb in ((1, bytes((255, 0, 0))), (3, bytes((0, 0, 255)))):
                image = content[index]
                url = image["image_url"]["url"] if protocol == "chat" else image["image_url"]
                png = base64.b64decode(url.split(",", 1)[1], validate=True)
                self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
                offset, pixels = 8, None
                while offset < len(png):
                    size = struct.unpack(">I", png[offset:offset+4])[0]
                    kind = png[offset+4:offset+8]
                    data = png[offset+8:offset+8+size]
                    crc = struct.unpack(">I", png[offset+8+size:offset+12+size])[0]
                    self.assertEqual(crc, zlib.crc32(kind+data) & 0xFFFFFFFF)
                    if kind == b"IHDR":
                        self.assertEqual(struct.unpack(">IIBBBBB", data), (192,192,8,2,0,0,0))
                    if kind == b"IDAT":
                        pixels = zlib.decompress(data)
                    offset += size + 12
                self.assertEqual(pixels, (b"\x00"+rgb*192)*192)

    def test_visual_reasoning_matrix_budget_and_pixel_counts_have_independent_oracles(self):
        import base64
        import struct
        import zlib
        from probe_support.scenarios import plan_groups, expect_visual_math
        from probe_support.images import visual_math_history
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp)/"run",providers="xiaomi",
                models=["mimo-v2.6-pro"],limit=24,tokens=2048)
            groups = [group for effort in ("none","minimal","medium")
                for group in plan_groups(run,run.plan["models"],cases=("image","image_math"),effort=effort)]
            self.assertEqual(sum(group[5] for group in groups),24)
            self.assertEqual(len(set(group[4] for group in groups)),24)
            self.assertTrue(all(group[6]==(512 if group[3]=="image" else 2048) for group in groups))
        for protocol in ("chat","responses"):
            content = visual_math_history(protocol)[0]["content"]
            self.assertEqual(len(content),4)
            counts = []
            for index in (1,3):
                url = content[index]["image_url"]
                if protocol=="chat": url=url["url"]
                png = base64.b64decode(url.split(",",1)[1],validate=True)
                offset, data = 8, bytearray()
                while offset<len(png):
                    size = struct.unpack(">I",png[offset:offset+4])[0]
                    if png[offset+4:offset+8]==b"IDAT": data.extend(png[offset+8:offset+8+size])
                    offset += size+12
                pixels = zlib.decompress(data)
                rgb = [pixels[row*(1+192*3)+1:row*(1+192*3)+1+192*3] for row in range(192)]
                colors = [rgb[y*64+32][(x*64+32)*3:(x*64+32)*3+3] for y in range(3) for x in range(3)]
                counts.append((colors.count(bytes((255,0,0))),colors.count(bytes((0,0,255)))))
            self.assertEqual(counts,[(5,4),(3,6)])
            answer=counts[0][0]*counts[1][1]-counts[0][1]*counts[1][0]
            self.assertEqual(answer,18)
            expect_visual_math(json.dumps({"answer":answer}),[],[])
        for text,code in (('{"answer":17}','visual_math_value'),
            ('{"answer":"18"}','visual_math_format'),('{"answer":18,"extra":0}','visual_math_format'),
            ('{"answer":17,"answer":18}','visual_math_format')):
            with self.assertRaises(ProbeFailure) as raised: expect_visual_math(text,[],[])
            self.assertEqual(raised.exception.code,code)

    def test_explicit_max_vision_control_keeps_preset_and_unknown_efforts_closed(self):
        from probe_support.scenarios import plan_groups
        with tempfile.TemporaryDirectory() as temp:
            run=Run.create(Path(temp)/"run",providers="openrouter",models=["gpt-6-luna"],limit=8,tokens=2048)
            groups=plan_groups(run,run.plan["models"],cases=("image","image_math"),effort="max")
            self.assertEqual(sum(g[5] for g in groups),8)
            self.assertTrue(all(g[4].endswith(":max") for g in groups))
            for cases,effort in ((("reasoning",),"max"),(("image",),"unknown")):
                with self.assertRaises(ProbeFailure):
                    plan_groups(run,run.plan["models"],cases=cases,effort=effort)

    def test_plan_cannot_be_mutated_in_memory_and_expiry_preserves_readback(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", limit=1)
            view = run.plan
            view["limit"] = 99
            view["models"].append("kimi-k3")
            self.assertEqual(run.plan["limit"], 1)
            self.assertNotIn("kimi-k3", run.plan["models"])
            identity = run.reserve("nemotron-3-super", "first", 8)
            with patch(
                "probe_support.ledger.time.time", return_value=run.plan["expires"] + 1
            ):
                run.finish(identity, "passed", {"content_ok": True})
                self.assertEqual(Run(run.directory).snapshot()[0]["state"], "passed")
                with self.assertRaises(ProbeFailure):
                    run.reserve("nemotron-3-super", "later", 8)

    def test_cross_process_reservations_crash_and_changed_plan_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", limit=3)
            with concurrent.futures.ProcessPoolExecutor(max_workers=4) as pool:
                ids = list(pool.map(reserve_worker, [str(run.directory)] * 8, range(8)))
            self.assertEqual(sum(x is not None for x in ids), 3)
            self.assertEqual(len(Run(run.directory).snapshot()), 3)
            with self.assertRaises(ProbeFailure):
                run.reserve("nemotron-3-super", "another", 8)
            path = run.directory / "plan.json"
            plan = json.loads(path.read_text())
            plan["limit"] = 20
            path.write_text(json.dumps(plan))
            with self.assertRaises(ProbeFailure):
                Run(run.directory)

    def test_pending_is_not_replayed_and_reports_cannot_leak_payloads(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Run.create(Path(temp) / "run", limit=3, continue_oracle=True)
            identity = run.reserve("nemotron-3-super", "first", 8)
            with self.assertRaises(ProbeFailure):
                Run(run.directory).reserve("nemotron-3-super", "first", 8)
            with self.assertRaises(RuntimeError):
                run.finish(identity, "failed", {"body": "synthetic-private"})
            run.dispatched(identity)
            run.finish(identity, "oracle_failed", {"failure": "oracle"})
            identity = run.reserve("nemotron-3-super", "second", 8)
            run.finish(identity, "failed", {"http": 429, "retry_after_seconds": 7})
            with self.assertRaises(ProbeFailure):
                run.reserve("nemotron-3-super", "third", 8)

    def test_sse_requires_unique_finish_done_and_no_data_after_it(self):
        finish = 'data: {"choices":[{"index":0,"delta":{"content":"pong"},"finish_reason":"stop"}]}\n\n'
        wire = Wire("chat", True)
        for byte in (finish + "data: [DONE]\n\n").encode():
            wire.push(bytes([byte]))
        wire.finish()
        self.assertTrue(wire.closed)
        self.assertEqual(wire.text, "pong")
        for raw in (
            finish,
            finish + finish + "data: [DONE]\n\n",
            "data: [DONE]\n\n",
            finish + "data: [DONE]\n\ndata: {}\n\n",
        ):
            with self.assertRaises(ProbeFailure):
                value = Wire("chat", True)
                value.push(raw.encode())
                value.finish()
            with self.assertRaises(ProbeFailure):
                value.finish()
            self.assertFalse(value.closed)
        with self.assertRaises(ProbeFailure):
            Wire("chat", True, limit=8).push(b"x" * 9)

    def test_checks_survive_optimized_python(self):
        root = Path(__file__).resolve().parents[2]
        code = "import sys;sys.path.insert(0,'examples');from probe_support.scenarios import expect_text;expect_text('42')('142',[],[])"
        result = subprocess.run(
            [sys.executable, "-O", "-c", code], cwd=root, capture_output=True
        )
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
