// Actual gateway discovery -> native FileModelsStore -> selected SDK session.
// The separate RPC process restores this same catalog with the gateway stopped.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [packageDir, extension, directory] = process.argv.slice(2).map(path => resolve(path));
const load = name => import(pathToFileURL(join(packageDir, "dist/core", `${name}.js`)));
const { loadExtensions } = await load("extensions/loader");
const { ModelRuntime } = await load("model-runtime");
const { FileModelsStore } = await load("models-store");
const { DefaultResourceLoader } = await load("resource-loader");
const { SessionManager } = await load("session-manager");
const { SettingsManager } = await load("settings-manager");
const { createAgentSession } = await load("sdk");
const agentDir = join(directory, "pi-agent");
const expected = JSON.parse(readFileSync(join(directory, "expected.json"), "utf8"));
const loaded = await loadExtensions([extension], directory);
assert.deepEqual(loaded.errors, []);
const runtime = await ModelRuntime.create({
  authPath: join(agentDir, "auth.json"), modelsPath: join(agentDir, "models.json"),
  modelsStore: new FileModelsStore(join(agentDir, "models-store.json")),
});
for (const registration of loaded.runtime.pendingNativeProviderRegistrations) {
  runtime.registerNativeProvider(registration.provider);
}
const result = await runtime.refresh({ providers: ["morphiecore"], allowNetwork: true, force: true,
  signal: AbortSignal.timeout(10000) });
assert.equal(result.aborted, false);
assert.equal(result.errors.size, 0);
const settingsManager = SettingsManager.create(directory, agentDir);
const resources = new DefaultResourceLoader({
  cwd: directory, agentDir, settingsManager, noExtensions: true, noSkills: true,
  noPromptTemplates: true, noThemes: true, noContextFiles: true,
  extensionsOverride: () => loaded, systemPrompt: "Synthetic context metadata inspection.",
});
await resources.reload();
const rows = [];
for (const [id, { contextWindow, maxTokens, input }] of Object.entries(expected)) {
  const model = runtime.getModel("morphiecore", id);
  assert(model, `discovered model ${id}`);
  assert.equal(model.contextWindow, contextWindow, `registry context ${id}`);
  assert.equal(model.maxTokens, maxTokens, `registry output ${id}`);
  assert.deepEqual(model.input, input, `registry image carriage ${id}`);
  if (id.toLowerCase().includes("minicpm")) {
    assert.equal(model.reasoning, true, `thinking available ${id}`);
    assert.equal(model.thinkingLevelMap.off, "none", `thinking off ${id}`);
    assert.equal(model.thinkingLevelMap.medium, "medium", `thinking on ${id}`);
  }
  if (id === "hy4" || id === "minimax-m3.1-flash") {
    assert.equal(model.reasoning, true, `Go thinking available ${id}`);
    assert.equal(model.thinkingLevelMap.high, "high", `Go native high ${id}`);
    if (id === "hy4") {
      assert.equal(model.thinkingLevelMap.off, "none", "HY4 thinking off");
      assert.equal(model.thinkingLevelMap.medium, null, "HY4 aliases hidden");
    }
  }
  const { session } = await createAgentSession({ cwd: directory, agentDir, modelRuntime: runtime,
    model, settingsManager, resourceLoader: resources, noTools: "all",
    sessionManager: SessionManager.inMemory(directory), thinkingLevel: "low" });
  try {
    assert.equal(session.model.contextWindow, contextWindow, `selected context ${id}`);
    assert.equal(session.model.maxTokens, maxTokens, `selected output ${id}`);
    assert.deepEqual(session.model.input, input, `selected image carriage ${id}`);
    assert.equal(session.getContextUsage().contextWindow, contextWindow, `session context accounting ${id}`);
    rows.push({ id, contextWindow, maxTokens });
  } finally { session.dispose(); }
}
const stored = JSON.parse(readFileSync(join(agentDir, "models-store.json"), "utf8"));
for (const [id, { contextWindow, maxTokens, input }] of Object.entries(expected)) {
  assert.equal(stored.morphiecore.models.find(model => model.id === id)?.contextWindow,
    contextWindow, `native persistent context ${id}`);
  assert.equal(stored.morphiecore.models.find(model => model.id === id)?.maxTokens,
    maxTokens, `native persistent output ${id}`);
  assert.deepEqual(stored.morphiecore.models.find(model => model.id === id)?.input,
    input, `native persistent image carriage ${id}`);
}
await runtime.settleCredentialWrites();
console.log(JSON.stringify({ discovered: rows }));
