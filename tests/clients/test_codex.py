"""Independent unit oracles for the Codex CLI loopback driver."""
import copy
import json
import unittest

import codex


class CodexDriverOracles(unittest.TestCase):
    def sample_responses_request(self):
        return {
            "model": "gpt-6.2-sol",
            "input": [
                {
                    "type": "additional_tools",
                    "tools": [
                        {
                            "type": "namespace",
                            "name": "functions",
                            "description": "",
                            "tools": [
                                {
                                    "type": "custom",
                                    "name": "exec",
                                    "description": "Execute JS",
                                    "format": {"type": "text"},
                                }
                            ],
                        }
                    ],
                },
                {"role": "user", "content": "run"},
            ],
            "tool_choice": "auto",
        }

    def sample_cross_protocol_request(self):
        return {
            "model": "claude-haiku-5-5",
            "input": [{"role": "user", "content": "run"}],
            "tools": [
                {
                    "type": "namespace",
                    "name": "multi_agent_v1",
                    "description": "Multi-agent tools",
                    "tools": [
                        {
                            "type": "function",
                            "name": "close_agent",
                            "description": "Close an agent",
                            "parameters": {
                                "type": "object",
                                "properties": {"id": {"type": "string"}},
                                "required": ["id"],
                            },
                        }
                    ],
                }
            ],
            "tool_choice": "auto",
        }

    def test_responses_state_requires_unqualified_custom_call_and_rejects_bad_namespace(self):
        state = codex.CodexState("codex-responses")
        turn1 = self.sample_responses_request()
        state.incoming({"authorization": f"Bearer {codex.KEY}"}, turn1)
        status, _, _ = state.upstream(
            {"authorization": "Bearer synthetic-client-primary"}, turn1
        )
        self.assertEqual(status, 200)

        turn2 = copy.deepcopy(turn1)
        turn2["input"].extend(
            [
                {
                    "type": "custom_tool_call",
                    "call_id": codex.CODEX_CALL,
                    "name": "exec",
                    "input": codex.CODE_MODE_PAYLOAD,
                },
                {
                    "type": "custom_tool_call_output",
                    "call_id": codex.CODEX_CALL,
                    "output": "ok",
                },
            ]
        )
        state.incoming({"authorization": f"Bearer {codex.KEY}"}, turn2)
        status, _, _ = state.upstream(
            {"authorization": "Bearer synthetic-client-primary"}, turn2
        )
        self.assertEqual(status, 200)

        bad_turn2 = copy.deepcopy(turn2)
        bad_turn2["input"][2]["namespace"] = "other"
        with self.assertRaisesRegex(RuntimeError, "^codex_turn2_missing_unqualified_custom_call$"):
            codex.CodexState("codex-responses").incoming(
                {"authorization": f"Bearer {codex.KEY}"}, bad_turn2
            )

    def test_cross_protocol_messages_requires_alias_and_realiased_turn2(self):
        aliased_tool = {
            "name": "mcns_0123456789ab_multi_agent_v1_close_agent",
            "description": "[namespace: multi_agent_v1]\n\nClose an agent",
            "input_schema": {
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
            },
        }
        state = codex.CodexState("codex-messages")
        turn1_in = self.sample_cross_protocol_request()
        state.incoming({"authorization": f"Bearer {codex.KEY}"}, turn1_in)
        status, _, _ = state.upstream(
            {"x-api-key": "synthetic-client-primary"},
            {"messages": [{"role": "user", "content": "run"}], "tools": [aliased_tool]},
        )
        self.assertEqual(status, 200)

        turn2_in = copy.deepcopy(turn1_in)
        turn2_in["input"].extend(
            [
                {
                    "type": "function_call",
                    "call_id": codex.NS_CALL,
                    "name": "close_agent",
                    "namespace": "multi_agent_v1",
                    "arguments": json.dumps(codex.NS_ARGS),
                },
                {
                    "type": "function_call_output",
                    "call_id": codex.NS_CALL,
                    "output": "done",
                },
            ]
        )
        state.incoming({"authorization": f"Bearer {codex.KEY}"}, turn2_in)
        status, _, _ = state.upstream(
            {"x-api-key": "synthetic-client-primary"},
            {
                "messages": [
                    {"role": "user", "content": "run"},
                    {
                        "role": "assistant",
                        "content": [
                            {
                                "type": "tool_use",
                                "id": codex.NS_CALL,
                                "name": aliased_tool["name"],
                                "input": codex.NS_ARGS,
                            }
                        ],
                    },
                    {
                        "role": "user",
                        "content": [
                            {"type": "tool_result", "tool_use_id": codex.NS_CALL, "content": "done"}
                        ],
                    },
                ],
                "tools": [aliased_tool],
            },
        )
        self.assertEqual(status, 200)

        leaked = {"messages": [{"role": "user", "content": "run"}], "tools": [aliased_tool, {"type": "custom", "name": "sql"}]}
        with self.assertRaisesRegex(RuntimeError, "^messages_leaked_namespace_or_custom$"):
            codex.CodexState("codex-messages").upstream(
                {"x-api-key": "synthetic-client-primary"}, leaked
            )

    def test_chat_events_and_version_validation(self):
        codex.check_client_version("codex-responses", "codex-cli 0.172.0")
        with self.assertRaisesRegex(RuntimeError, "^invalid_client_version$"):
            codex.check_client_version("codex-responses", "codex-cli invalid")
        payload = codex.chat_events(1, "mcns_0123456789ab_multi_agent_v1_close_agent", "claude-haiku-5-5").decode()
        chunks = [json.loads(line[6:]) for line in payload.splitlines() if line.startswith("data: ") and line != "data: [DONE]"]
        self.assertEqual(
            json.loads(chunks[1]["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]),
            codex.NS_ARGS,
        )


if __name__ == "__main__":
    unittest.main()
