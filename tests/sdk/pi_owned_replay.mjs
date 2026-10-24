// Actual pinned pi parsing/building; no sockets, credentials, retries or tool execution.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
const pins = JSON.parse(readFileSync(new URL('../upstream/sources.lock.json', import.meta.url), 'utf8'));
assert.equal(execFileSync('git', ['-C', process.argv[2], 'rev-parse', 'HEAD'],
  {encoding:'utf8'}).trim(), pins.sources.pi.revision, 'Use the pinned stable pi source');
globalThis.fetch = async () => { throw new Error('Unexpected external fetch'); };
const cases = JSON.parse(readFileSync(0, 'utf8'));
assert.equal(cases.length, 8);
const returned = [];
for (const fixture of cases) {
  const api = fixture.api;
  const { stream } = await import(pathToFileURL(resolve(process.argv[2], `packages/ai/src/api/${api}.ts`)));
  const model = { type: 'chat', id: 'public', name: 'public', api, provider: 'synthetic',
    baseUrl: 'https://fixture.invalid/v1', input: ['text'], reasoning: true,
    contextWindow: 4096, maxTokens: 32, cost: {input:0,output:0,cacheRead:0,cacheWrite:0} };
  const requests = [];
  const fetch = async (url, init) => {
    assert.equal(new URL(typeof url === 'string' ? url : url.url ?? url.toString()).hostname, 'fixture.invalid');
    assert(requests.length < 2, 'No retries or excess requests');
    requests.push(JSON.parse(init.body));
    return new Response(fixture.wire, {status:200,headers:{'Content-Type':'text/event-stream'}});
  };
  const options = { apiKey:'synthetic-placeholder',env:{},fetch,maxTokens:32,maxRetries:0,
    cacheRetention:'none',signal:AbortSignal.timeout(5000) };
  const context = { messages:[{role:'user',content:'go',timestamp:0}],
    tools:[{name:'lookup',description:'Synthetic fixture',parameters:{type:'object',properties:{n:{type:'integer'}}}}] };
  const first = await stream(model,context,options).result();
  assert.notEqual(first.stopReason,'error',first.errorMessage);
  assert.equal(first.content.filter(b=>b.type==='text').map(b=>b.text).join(''),
    fixture.phase ? 'prefixanothercheckingdone' : fixture.empty ? '' : 'OKOK');
  const call = first.content.find(b=>b.type==='toolCall');
  assert.equal(Boolean(call),!fixture.empty && !fixture.phase);
  const original = structuredClone(first);
  const followup = call
    ? {role:'toolResult',toolCallId:call.id,toolName:'lookup',content:[{type:'text',text:'ok'}],isError:false,timestamp:1}
    : {role:'user',content:'continue',timestamp:1};
  const next = await stream(model,{...context,messages:[...context.messages,first,followup]},options).result();
  assert.notEqual(next.stopReason,'error',next.errorMessage);
  assert.deepEqual(first,original);
  assert.equal(requests.length,2);
  returned.push(requests[1]);
}
process.stdout.write(JSON.stringify(returned));
