"""Installed pi: real read tool, stateless native Google signatures and session resume."""
from contextlib import ExitStack
import json
from pathlib import Path
import queue
import signal
import sys

from drivers import (Child, TEXT, Pi, check, check_client_version, environment,
                     gateway_configuration, pi_configuration)
from recovery import State as RecoveryState, server

MODEL = "gemini-3.8-flash"


class State(RecoveryState):
    def __init__(self, path):
        super().__init__(False, path)
        self.endpoint_path = "/v1/responses"
        self.upstream_endpoint_path = f"/v1beta/models/{MODEL}:streamGenerateContent"
        self.upstream_query = "alt=sse"
        self.parts = [
            {"text": "Inspecting.", "thoughtSignature": "dGV4dC0x"},
            {"functionCall": {"id": "c1", "name": "read", "args": {"path": str(path)}},
             "thoughtSignature": "Y2FsbC0x"},
        ]
        self.final = [{"text": TEXT, "thoughtSignature": "ZmluYWwtMQ=="},
                      {"text": "!", "thoughtSignature": "ZmluYWwtMg=="}]

    def incoming(self, headers, body):
        check(headers.get_all("x-morphiecore-responses-profile") == ["owner-replay-v1"],
              "owner_profile_selection")
        check(body.get("store") is False and "previous_response_id" not in body,
              "owner_stateless_request")
        with self.lock:
            check(len(self.ingress) < 3, "owner_client_request_budget")
            ordinal = len(self.ingress)
            self.ingress.append(ordinal)
        owners = [item for item in body["input"] if "morphiecore_replay" in item]
        check(len(owners) == [0, 2, 4][ordinal], "owner_persisted_attachment_count")

    def upstream(self, headers, body):
        check(headers.get("x-goog-api-key") == "synthetic-client-primary"
              and headers.get("authorization") is None, "owner_upstream_auth")
        with self.lock:
            ordinal = len(self.attempts)
            check(ordinal < 3, "owner_upstream_request_budget")
            self.attempts.append(ordinal)
        history = body["contents"]
        signed = [part for item in history if item["role"] == "model" for part in item["parts"]]
        check(signed == ([] if ordinal == 0 else self.parts if ordinal == 1
                         else self.parts + self.final), "owner_native_signed_history")
        if ordinal > 0:
            results = [part["functionResponse"] for item in history for part in item["parts"]
                       if "functionResponse" in part]
            check(len(results) == 1 and results[0]["id"] == "c1"
                  and TEXT in results[0]["response"]["output"], "owner_real_read_result")
        check("morphiecore_replay" not in json.dumps(body), "owner_extension_not_upstream")
        parts = [self.parts, self.final, [{"text": "continued"}]][ordinal]
        # Optional responseId is deliberately absent throughout the actual Agent loop.
        frame = {"modelVersion": MODEL, "candidates": [
            {"content": {"role": "model", "parts": parts}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 3, "totalTokenCount": 13}}
        return 200, ("data: " + json.dumps(frame) + "\n\n").encode(), "text/event-stream"


def main():
    case, directory, gateway_binary, binary, source = sys.argv[1:]
    check(Path(source).is_absolute() and Path(source).is_file(), "owner_extension_source")
    root = Path(directory)
    work = root / "work"
    work.mkdir(mode=0o700)
    path = work / "sample.txt"
    path.write_text(TEXT + "\n", encoding="utf-8")
    state = State(path)
    env = environment(root)
    with Child([binary, "--version"], env, work) as client:
        version = client.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    with ExitStack() as stack:
        origin = stack.enter_context(server(state, False))
        gateway_configuration(root, False, origin, model=MODEL, provider="google",
                              binding=f"google-{MODEL}-generate-content", api_root="/v1beta")
        gateway = stack.enter_context(Child([gateway_binary, "--config", str(root / "gateway.json"),
            "--credentials-dir", str(root / "credentials")], env, work))
        try:
            ready = gateway.first_line.get(timeout=5).decode()
        except queue.Empty:
            raise RuntimeError("owner_gateway_readiness") from None
        check(len(ready) <= 127 and ready.startswith("MorphieCore listening on http://127.0.0.1:"),
              "owner_gateway_startup")
        state.gateway = ready.removeprefix("MorphieCore listening on ")
        ingress = stack.enter_context(server(state, True))
        agent, _ = pi_configuration(root, ingress, False, model=MODEL)
        env["PI_CODING_AGENT_DIR"] = str(agent)
        extension = root / "owner-replay.ts"
        # Real public provider adapter and stock serializer; no transport or Agent mock.
        extension.write_text(
            f'import {{ ownerReplay }} from {json.dumps(source)};\n'
            'import { openAIResponsesApi } from "@earendil-works/pi-ai/compat";\n'
            'export default pi => pi.registerProvider("morphiecore", {\n'
            f'baseUrl: {json.dumps(ingress + "/v1")}, apiKey: "$INGRESS_KEY",\n'
            'api: "openai-responses", streamSimple: ownerReplay(openAIResponsesApi()).streamSimple,\n'
            f'models: [{{id: {json.dumps(MODEL)}, name: "Synthetic Google", reasoning: true, input: ["text"],'
            'contextWindow: 128000, maxTokens: 256, cost: {input:0,output:0,cacheRead:0,cacheWrite:0},'
            'compat: {supportsStrictMode:false,supportsDeveloperRole:false,supportsLongCacheRetention:false}}]});\n',
            encoding="utf-8")
        with Pi(binary, env, work, MODEL, provider="morphiecore",
                extensions=[extension], persist=True) as client:
            client.prompt("Read.", ["toolUse", "stop"])
            info, _ = client.command("get_state")
            session = info["sessionFile"]
            check(Path(session).is_file() and Path(session).is_relative_to(agent), "owner_session_path")
        with Pi(binary, env, work, MODEL, provider="morphiecore",
                extensions=[extension], session=session) as client:
            client.prompt("Continue.", ["stop"],
                          check_output=lambda content: check(any(
                              part.get("text") == "continued" for part in content), "owner_resumed_answer"))
        gateway.process.send_signal(signal.SIGINT)
        gateway.wait(timeout=5)
    check(not state.errors and len(state.ingress) == len(state.attempts) == 3,
          "owner_exchange_oracle:" + ",".join(state.errors))
    print(json.dumps({"case": case, "version": version, "client_requests": 3,
        "upstream_attempts": 3, "extra_client_requests": 0, "real_read": True,
        "session_resume": True, "signed_owner_history": True, "optional_response_id": True}))


if __name__ == "__main__":
    main()
