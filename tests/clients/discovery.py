"""Current Pi consumes the real gateway directory and restores native planning metadata."""
from contextlib import closing
import http.client
import json
from pathlib import Path
import queue
import sys

from drivers import (Child, KEY, Pi, check, check_client_version, environment,
                     private_json, shutdown_gateway)


def main():
    case, directory, gateway_binary, binary, extension = sys.argv[1:]
    root = Path(directory)
    agent = root / "pi-agent"
    agent.mkdir(mode=0o700)
    env = environment(root)
    env.update(MORPHIECORE_API_KEY=KEY, PI_CODING_AGENT_DIR=str(agent), PI_OFFLINE="1")
    with Child([binary, "--version"], env, root) as process:
        version = process.wait(timeout=5).decode().strip()
    check_client_version(case, version)
    domains = ["deepseek", "google", "opencode-go", "modelbest",
               "aliyun-dashscope-cn", "aliyun-tokenplan-cn", "openrouter", "xiaomi"]
    private_json(root / "accounts.json", {"version": 1, "providers": {
        domain: {"accounts": [{"name": "primary", "enabled": True,
                               "credential": {"alias": "primary"}}]}
        for domain in domains}})
    private_json(root / "auth.json", {"version": 1, "agents": [
        {"agent_id": "fixture-agent", "key": KEY}]})
    bindings = {
        "deepseek-v4.1-flash-1017": ["deepseek-responses"],
        "claude-haiku-5-5": ["opencode-go-haiku-messages"],
        "muse-spark-1.3-contributor": ["opencode-go-muse-responses"],
        "hy4-preview": ["opencode-go-chat"],
        "mimo-v2.6-flash": ["opencode-go-mimo-flash-chat"],
        "mimo-v2.6-pro": ["opencode-go-mimo-pro-chat", "xiaomi-responses"],
        "gpt-6-luna": ["openrouter-responses"],
        "gpt-audio-mini": ["openrouter-audio-chat"],
        "minicpm5-1b": ["modelbest-minicpm5-1b-chat"],
        "minicpm5-2b": ["modelbest-minicpm5-2b-chat"],
        "MiniCPM-V-4.7-1B": ["modelbest-minicpm-v-4.7-1b-chat"],
        "MiniCPM-V-4.7-35B-A3B": ["modelbest-minicpm-v-4.7-35b-a3b-chat"],
        "gemini-3.8-flash": ["google-gemini-3.8-flash-generate-content"],
        "kimi-k3": ["dashscope-kimi-chat"],
        # A known native capacity must not fill an unknown subscription candidate.
        "qwen3.8-flash": ["dashscope-flash-responses", "aliyun-tokenplan-cn-responses"],
    }
    private_json(root / "models.json", {"version": 1, "models": {
        model: {"candidates": [{"binding": binding} for binding in selected]}
        for model, selected in bindings.items()}})
    private_json(root / "gateway.json", {"version": 1, "bind": "127.0.0.1:0",
        "accounts_file": "accounts.json", "auth_file": "auth.json", "models_file": "models.json",
        "credentials_dir": str(root / "credentials"), "oauth": {"renewal_timeout_ms": 0}})
    gateway = Child([gateway_binary, "--config", str(root / "gateway.json")], env, root)
    failed = False
    try:
        try:
            ready = gateway.first_line.get(timeout=10).decode()
        except queue.Empty:
            raise RuntimeError("discovery_gateway_readiness") from None
        prefix = "MorphieCore listening on http://127.0.0.1:"
        check(ready.startswith(prefix), "discovery_gateway_loopback")
        port = int(ready.removeprefix(prefix))
        with closing(http.client.HTTPConnection("127.0.0.1", port, timeout=5)) as connection:
            connection.request("GET", "/v1/morphiecore/models", headers={"authorization": "Bearer " + KEY})
            response = connection.getresponse()
            data = response.read((1 << 20) + 1)
            check(response.status == 200 and len(data) <= 1 << 20, "discovery_document")
            document = json.loads(data)
        rows = {row["id"]: next(iface["generation"] for iface in row["interfaces"]
                if iface["protocol"] == "responses") for row in document["data"]}
        check(set(rows) == set(bindings), "public_model_identity")
        deepseek = rows["deepseek-v4.1-flash-1017"]["token_limits"]
        check(deepseek["context_window_tokens"] == 1048576, "native_service_capacity_missing")
        check(rows["gemini-3.8-flash"]["token_limits"]["context_window_tokens"] is None
              and rows["gemini-3.8-flash"]["token_limits"]["max_input_tokens"] == 1048576,
              "input_capacity_is_not_total")
        check(rows["qwen3.8-flash"]["token_limits"]["context_window_tokens"] is None,
              "unknown_candidate_is_not_native_capacity")
        check(rows["mimo-v2.6-pro"]["token_limits"]["max_output_tokens"] is None,
              "unknown_candidate_is_not_go_output_capacity")
        for model, levels in [("hy4-preview", ["high"]),
                              ("muse-spark-1.3-contributor", ["minimal", "low", "medium", "high", "xhigh"])]:
            check(rows[model]["reasoning"]["effort"]["reasoning"]["values"] == levels,
                  "go_native_effort_domain")
        for model in bindings:
            if "minicpm" in model.lower():
                reasoning = rows[model]["reasoning"]
                check("disabled" in reasoning["activation"]["values"],
                      "minicpm_thinking_off")
                check("medium" in reasoning["effort"]["reasoning"]["values"],
                      "minicpm_standard_effort_carrier")
        policy = {"baseUrl": f"http://127.0.0.1:{port}", "protocols": ["responses"],
                  "contextWindow": 131072, "maxTokens": 2048, "timeoutSeconds": 5,
                  "modelOverrides": {"MiniCPM-V-4.7-1B": {"contextWindow": 200000},
                                     "hy4-preview": {"maxTokens": 48000}}}
        expected = {}
        for model, generation in rows.items():
            limits = generation["token_limits"]
            window = limits["context_window_tokens"] or limits["max_input_tokens"] or policy["contextWindow"]
            override = policy["modelOverrides"].get(model, {})
            window = min(window, override.get("contextWindow", window))
            output = limits["max_output_tokens"] or override.get("maxTokens", policy["maxTokens"])
            expected[model] = {"contextWindow": window,
                               "maxTokens": min(output, override.get("maxTokens", output), window)}
        private_json(root / "expected.json", expected)
        private_json(root / "policy.json", {"morphiecore": policy})
        env["PI_EXTENSIONS_CONFIG"] = str(root / "policy.json")
        private_json(agent / "models.json", {"providers": {"morphiecore": {"apiKey": "$MORPHIECORE_API_KEY"}}})
        private_json(agent / "settings.json", {"cacheWarming": "off", "retry": {"enabled": False},
                                                "compaction": {"enabled": False}})
        package = Path(binary).resolve().parents[1] / "lib/pi/node_modules/@earendil-works/pi-coding-agent"
        with Child(["node", str(Path(__file__).with_suffix(".mjs")), str(package), extension, str(root)], env, root) as process:
            seeded = json.loads(process.wait(timeout=40))
        check(len(seeded["discovered"]) == len(expected), "native_refresh_count")
    except Exception:
        failed = True
        raise
    finally:
        shutdown_gateway(gateway, failed=failed)
    # The real RPC client must restore the native store without contacting a gateway.
    with Pi(binary, env, root, "deepseek-v4.1-flash-1017", provider="morphiecore",
            extensions=[extension]) as client:
        available, _ = client.command("get_available_models")
        actual = {model["id"]: {key: model[key] for key in ["contextWindow", "maxTokens"]}
                  for model in available["models"]
                  if model["provider"] == "morphiecore"}
        check(actual == expected, "offline_native_store_restore")
        for model, limits in expected.items():
            client.command("set_model", provider="morphiecore", modelId=model)
            state, _ = client.command("get_state")
            stats, _ = client.command("get_session_stats")
            check(all(state["model"][key] == value for key, value in limits.items()),
                  "selected_rpc_planning_limits")
            check(stats["contextUsage"]["contextWindow"] == limits["contextWindow"], "rpc_context_accounting")
    print(json.dumps({"case": case, "version": version, "status": "passed",
                      "models": actual}), flush=True)


if __name__ == "__main__":
    main()
