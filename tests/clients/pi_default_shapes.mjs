// Capture synthetic default HTTP bodies from the currently installed public pi-ai package.
// Usage: node tests/clients/pi_default_shapes.mjs /absolute/path/to/pi-ai
// No network, personal settings, auth caches or real keys; output is a reviewed fixture input.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = resolve(process.argv[2]);
const manifest = JSON.parse(readFileSync(`${root}/package.json`, "utf8"));
assert.equal(manifest.name, "@earendil-works/pi-ai");
const { streamSimple } = await import(pathToFileURL(`${root}/dist/compat.js`));
const { getBuiltinModel } = await import(pathToFileURL(`${root}/dist/providers/all.js`));
const result = { source: `${manifest.name}@${manifest.version}`, requests: {} };
for (const [api, name] of [
  ["openai-completions", "chat"],
  ["openai-responses", "responses"],
  ["anthropic-messages", "messages"],
]) {
  const corpus = JSON.parse(readFileSync(new URL(`../../testdata/repair-and-recall/${name}/session.json`, import.meta.url), "utf8"));
  result.requests[name] = {};
  for (const profile of ["text", "tools", "reasoning", ...(name === "messages" ? ["raw_headers"] : [])]) {
    const rawHeaders = profile === "raw_headers";
    const reasoning = profile === "reasoning" || rawHeaders;
    const model = {
      id: "synthetic-model", name: "Synthetic", api, provider: "synthetic",
      baseUrl: "http://127.0.0.1:1/v1", reasoning, input: ["text"],
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
      reasoning: reasoning ? "medium" : undefined,
      maxRetries: 0, timeoutMs: 2000,
      headers: api === "anthropic-messages" && !rawHeaders ? { "anthropic-version": "2026-10-21", "anthropic-beta": "" } : undefined,
      fetch: async (url, init) => {
        assert.equal(new URL(url).origin, "http://127.0.0.1:1");
        calls++;
        if (rawHeaders) {
          const headers = new Headers(init.headers);
          result.raw_messages_headers = Object.fromEntries(
            ["anthropic-version", "anthropic-beta"].map(name => [name, headers.get(name)])
          );
        } else {
          result.requests[name][profile] = JSON.parse(init.body);
        }
        return new Response(JSON.stringify({ error: { message: "Synthetic capture complete", type: "invalid_request_error" } }), {
          status: 400, headers: { "content-type": "application/json" },
        });
      },
    });
    assert.equal((await stream.result()).stopReason, "error");
    assert.equal(calls, 1);
  }
}
// Keep the full resolved public record; only redirect its origin to intercepted fetch.
const builtin = getBuiltinModel("opencode-go", "claude-haiku-5-5");
assert(builtin);
result.resolved_models = {
  opencode_go_haiku: {
    resolver: "getBuiltinModel",
    source_files: Object.fromEntries([
      "dist/providers/data/opencode-go.json",
      "dist/api/anthropic-messages.js",
    ].map(path => [path, createHash("sha256").update(readFileSync(`${root}/${path}`)).digest("hex")])),
    model: builtin,
    overrides: { baseUrl: "http://127.0.0.1:1/v1" },
    requests: {},
    headers: {},
  },
};
const resolved = result.resolved_models.opencode_go_haiku;
const corpus = JSON.parse(readFileSync(new URL("../../testdata/repair-and-recall/messages/session.json", import.meta.url), "utf8"));
for (const profile of ["text", "tools", "reasoning"]) {
  let calls = 0;
  const stream = streamSimple({ ...builtin, ...resolved.overrides }, {
    systemPrompt: "Repair the synthetic configuration.",
    messages: [{ role: "user", content: "Repair configuration; preserve naïve 🧪 and exact IDs.", timestamp: 0 }],
    ...(profile === "text" ? {} : {
      tools: corpus.client_request.tools.map(tool => ({
        name: tool.name,
        description: "Inspect or repair a synthetic configuration.",
        parameters: tool.input_schema,
      })),
    }),
  }, {
    apiKey: "synthetic-key", env: {}, sessionId: "synthetic-session",
    reasoning: profile === "reasoning" ? "medium" : undefined,
    maxRetries: 0, timeoutMs: 2000,
    fetch: async (url, init) => {
      assert.equal(new URL(url).origin, "http://127.0.0.1:1");
      calls++;
      resolved.requests[profile] = JSON.parse(init.body);
      const headers = new Headers(init.headers);
      resolved.headers[profile] = Object.fromEntries(
        ["anthropic-version", "anthropic-beta"].map(name => [name, headers.get(name)])
      );
      return new Response(JSON.stringify({ error: { message: "Synthetic capture complete", type: "invalid_request_error" } }), {
        status: 400, headers: { "content-type": "application/json" },
      });
    },
  });
  assert.equal((await stream.result()).stopReason, "error");
  assert.equal(calls, 1);
}
assert.equal(resolved.requests.reasoning.thinking.type, "adaptive");
assert.equal(resolved.requests.reasoning.output_config.effort, "medium");
assert(!Object.hasOwn(resolved.requests.reasoning, "temperature"));
console.log(JSON.stringify(result, null, 2));
