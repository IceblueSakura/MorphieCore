// Capture synthetic default HTTP bodies from the currently installed public pi-ai package.
// Usage: node tests/clients/pi_default_shapes.mjs /absolute/path/to/pi-ai
// No network, personal settings, auth caches or real keys; output is a reviewed fixture input.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = resolve(process.argv[2]);
const manifest = JSON.parse(readFileSync(`${root}/package.json`, "utf8"));
assert.equal(manifest.name, "@earendil-works/pi-ai");
const { streamSimple } = await import(pathToFileURL(`${root}/dist/compat.js`));
const result = { source: `${manifest.name}@${manifest.version}`, requests: {} };
for (const [api, name] of [
  ["openai-completions", "chat"],
  ["openai-responses", "responses"],
  ["anthropic-messages", "messages"],
]) {
  const corpus = JSON.parse(readFileSync(new URL(`../../testdata/repair-and-recall/${name}/session.json`, import.meta.url), "utf8"));
  result.requests[name] = {};
  for (const profile of ["text", "tools", "reasoning"]) {
    const model = {
      id: "synthetic-model", name: "Synthetic", api, provider: "synthetic",
      baseUrl: "http://127.0.0.1:1/v1", reasoning: profile === "reasoning", input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 200000, maxTokens: 16384,
    };
    const tools = corpus.client_request.tools.map(tool => ({
      name: (tool.function ?? tool).name,
      description: "Inspect or repair a synthetic configuration.",
      parameters: (tool.function ?? tool).parameters ?? tool.input_schema,
    }));
    const context = {
      systemPrompt: "Repair the synthetic configuration.",
      messages: [{ role: "user", content: "Repair configuration; preserve naïve 🧪 and exact IDs.", timestamp: 0 }],
      ...(profile === "text" ? {} : { tools }),
    };
    let calls = 0;
    const stream = streamSimple(model, context, {
      apiKey: "synthetic-key", env: {}, sessionId: "synthetic-session",
      reasoning: profile === "reasoning" ? "medium" : undefined,
      maxRetries: 0, timeoutMs: 2000,
      headers: api === "anthropic-messages" ? { "anthropic-version": "2026-10-21", "anthropic-beta": "" } : undefined,
      fetch: async (url, init) => {
        assert.equal(new URL(url).origin, "http://127.0.0.1:1");
        calls++;
        result.requests[name][profile] = JSON.parse(init.body);
        return new Response(JSON.stringify({ error: { message: "Synthetic capture complete", type: "invalid_request_error" } }), {
          status: 400, headers: { "content-type": "application/json" },
        });
      },
    });
    assert.equal((await stream.result()).stopReason, "error");
    assert.equal(calls, 1);
  }
}
console.log(JSON.stringify(result, null, 2));
