// Pinned SDK callbacks and untouched returned history over the existing loopback gate.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import Anthropic from '@anthropic-ai/sdk';

const expectedVersion = JSON.parse(readFileSync(new URL('./package.json', import.meta.url))).dependencies['@anthropic-ai/sdk'];
const actualVersion = JSON.parse(readFileSync(new URL('./node_modules/@anthropic-ai/sdk/package.json', import.meta.url))).version;
assert.equal(actualVersion, expectedVersion);
const baseURL = process.argv[2];
assert.equal(new URL(baseURL).hostname, '127.0.0.1');
const client = new Anthropic({ baseURL, apiKey: 'synthetic-local-token', maxRetries: 0, timeout: 10_000 });
const callbacks = { text: [], thinking: [], signature: [], inputJson: [], blocks: [] };
let active = null;
let previous = null;
let next = 0;
const stream = client.messages.stream({ model: 'schedule', max_tokens: 64, messages: [{ role: 'user', content: 'go' }] });
stream.on('streamEvent', (event) => {
  if (event.type === 'content_block_start') {
    assert.equal(active, null);
    assert.equal(event.index, next++);
    active = event.index;
  } else if (event.type === 'content_block_delta') {
    assert.equal(active, event.index);
  } else if (event.type === 'content_block_stop') {
    assert.equal(active, event.index);
    if (active === 0) assert.equal(previous.delta.type, 'signature_delta');
    active = null;
  }
  previous = event;
});
for (const name of ['text', 'thinking', 'signature', 'inputJson']) {
  stream.on(name, (value) => callbacks[name].push(value));
}
stream.on('contentBlock', (block) => callbacks.blocks.push(structuredClone(block)));
const message = await stream.finalMessage();
assert.equal(active, null);
assert.equal(next, 5);
assert.equal(message.stop_reason, 'tool_use');
assert.equal(callbacks.text.join(''), 'ACB');
assert.equal(callbacks.thinking.join(''), 'Plan');
assert.deepEqual(callbacks.signature, [message.content[0].signature]);
assert.ok(message.content[0].signature.startsWith('mc1.'));
assert.equal(callbacks.inputJson.join(''), ' \n{"n":1}{"n":2}');
assert.deepEqual(callbacks.blocks, message.content);
assert.deepEqual(message.content.slice(1), [
  { type: 'text', text: 'AC' }, { type: 'text', text: 'B' },
  { type: 'tool_use', id: 'first', name: 'lookup', input: { n: 1 } },
  { type: 'tool_use', id: 'second', name: 'lookup', input: { n: 2 } },
]);
const replayed = await client.messages.create({ model: 'schedule', max_tokens: 64, messages: [
  { role: 'user', content: 'go' }, { role: 'assistant', content: message.content },
  { role: 'user', content: [
    { type: 'tool_result', tool_use_id: 'second', content: 'two' },
    { type: 'tool_result', tool_use_id: 'first', content: 'one' },
  ] },
] });
assert.equal(replayed.content[0].text, 'replayed');
console.log('Messages TypeScript callbacks and returned history passed');
