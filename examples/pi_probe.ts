// Repository-owned Pi gate. No daily settings, auth cache, sessions or resources.
import { createServer } from 'node:http';
import { Readable } from 'node:stream';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join } from 'node:path';
import { WireObservation } from './provider_probe_observation.ts';
import { relayHeaders } from './probe_http_headers.ts';
import { ProbeSlots } from './probe_slot.ts';
import { record as object, array, protocol } from './probe_values.ts';
import { Type } from 'typebox';
import type { ResourceLoader } from '@earendil-works/pi-coding-agent';
process.on('uncaughtException', () => { console.error('Pi probe failed; private details suppressed.'); process.exit(1); });
const root = fileURLToPath(new URL('..',import.meta.url));
function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error('Missing explicit probe input');
  return value;
}
const run = requiredEnv('MORPHIECORE_PROBE_RUN');
const home = requiredEnv('MORPHIECORE_PI_HOME');
const pkg = requiredEnv('MORPHIECORE_PI_PACKAGE');
const version = object(JSON.parse(await readFile(join(pkg,'package.json'),'utf8'))).version;
if (version !== '1.0.4') throw new Error('Unpinned Pi');
const sdk: typeof import('@earendil-works/pi-coding-agent') = await import(pathToFileURL(join(pkg,'dist/index.js')).href);
const {createAgentSession,createExtensionRuntime,defineTool,ModelRuntime,SessionManager,SettingsManager} = sdk;
const plan = object(JSON.parse(await readFile(join(run,'plan.json'),'utf8')));
const tokens = plan.tokens;
if (typeof tokens !== 'number' || !Number.isSafeInteger(tokens) || tokens < 1 || tokens > 2048) throw new Error('Invalid plan budget');
const modelId = process.env.MORPHIECORE_TEST_MODEL;
if (!modelId || !array(plan.models).includes(modelId)) throw new Error('Unselected model');
const checking = process.env.MORPHIECORE_TEST_MODE === 'check';
const invalidAuth = process.env.MORPHIECORE_TEST_INVALID_AUTH === '1';
if (invalidAuth && !checking) throw new Error('Invalid auth requires synthetic mode');
const selectedProtocol = protocol(process.env.MORPHIECORE_TEST_PROTOCOL);
const responses = selectedProtocol === 'responses';
const thinking = process.env.MORPHIECORE_TEST_THINKING ?? 'off';
if (thinking !== 'off' && thinking !== 'minimal') throw new Error('Unselected effort');
const expectedEffort = thinking === 'off' ? (responses ? 'none' : undefined) : 'minimal';
const endpoint = responses ? '/v1/responses' : '/v1/chat/completions';
const upstream = process.env.MORPHIECORE_TEST_UPSTREAM;
if (!upstream || !/^http:\/\/127\.0\.0\.1:\d+$/.test(upstream)) throw new Error('Unowned destination');
interface AttemptRecord {
  attempt: string; case: string; tool_results: number;
  http?: number; observation?: WireObservation | null; failed?: boolean;
}
const slots = new ProbeSlots(checking ? 2 : 3);
const records: AttemptRecord[] = [], reports: {case: string; ok: boolean}[] = [];
let reads = 0, caseName = 'text';
function control(action: 'check' | 'register' | 'reserve' | 'dispatched' | 'finish', data: Record<string, unknown>) {
  const result = spawnSync(process.env.MORPHIECORE_PROBE_PYTHON ?? 'python3',[join(root,'examples/probe.py'),'control',run,action],
    {input:JSON.stringify(data),encoding:'utf8',maxBuffer:16384,timeout:10000});
  if (result.status !== 0) throw new Error('Probe ledger rejected operation');
  return result.stdout.trim();
}
const prefix = `pi-${checking ? (invalidAuth ? 'bad-auth' : 'check') : 'live'}:${modelId}:${selectedProtocol}:${thinking}`;
control('check',{model:modelId});
control('register',{cases:Array.from({length:checking ? 2 : 3},(_,n) => [modelId,`${prefix}:${n+1}`,tokens])});
const relay = createServer(async (req,res) => {
  let record: AttemptRecord | undefined;
  try {
    if (req.method !== 'POST' || req.url !== endpoint) { res.writeHead(404).end(); return; }
    const ordinal = slots.take(); // Before the first await; no racing reservations.
    const chunks: Buffer[] = []; let bytes = 0;
    for await (const chunk of req) {
      if (!Buffer.isBuffer(chunk)) throw new Error('Invalid request bytes');
      bytes += chunk.length;
      if (bytes > 256 * 1024) throw new Error('Request bytes');
      chunks.push(chunk);
    }
    const body = Buffer.concat(chunks), value = object(JSON.parse(body.toString('utf8')));
    const cap = responses ? value.max_output_tokens : value.max_completion_tokens;
    if (value.model !== modelId || value.stream !== true || (responses ? value.store !== false : 'store' in value) ||
        (responses ? object(value.reasoning ?? {}).effort !== expectedEffort : expectedEffort === undefined ? 'reasoning_effort' in value : value.reasoning_effort !== expectedEffort) ||
        typeof cap !== 'number' || !Number.isSafeInteger(cap) || cap < 1 || cap > tokens ||
        array(value.tools ?? []).map(object).some(t => {
          const tool = responses ? t : object(t.function);
          return tool.name !== 'read' || tool.strict !== false;
        })) throw new Error('Wire controls');
    const identity = control('reserve',{model:modelId,scenario:`${prefix}:${ordinal}`,tokens:cap});
    record = {attempt:identity,case:caseName,tool_results:responses ? array(value.input ?? []).map(object).filter(i => i.type === 'function_call_output').length : array(value.messages ?? []).map(object).filter(i => i.role === 'tool').length};
    records.push(record);
    const headers = relayHeaders(req.rawHeaders);
    headers['x-morphiecore-probe-id'] = identity;
    control('dispatched',{attempt:identity});
    const response = await fetch(upstream + endpoint,{method:'POST',headers,body,redirect:'error',signal:AbortSignal.timeout(130000)});
    record.http = response.status;
    res.writeHead(response.status,{'content-type':response.headers.get('content-type') ?? 'application/json'});
    if (!response.body) throw new Error('Missing response body');
    const source = Readable.fromWeb(response.body);
    const observation = response.ok ? new WireObservation(responses ? 'responses' : 'chat') : null;
    record.observation = observation;
    const expected = record.case === 'text' ? 'pong' : '42';
    const stream = Readable.from((async function* () {
      const iterator = source[Symbol.asyncIterator](); let size = 0;
      for await (const chunk of { [Symbol.asyncIterator]:() => iterator }) {
        size += chunk.length;
        if (size > 2 * 1024 * 1024) throw new Error('Response budget');
        observation?.push(chunk);
        if (observation?.facts(expected).wire_available) {
          for await (const tail of { [Symbol.asyncIterator]:() => iterator }) observation.push(tail);
          observation.finish();
          yield chunk;
          return;
        }
        yield chunk;
      }
      observation?.finish();
    })());
    stream.on('error',() => { if (record) record.failed = true; res.destroy(); });
    res.on('close',() => { stream.destroy(); source.destroy(); });
    stream.pipe(res);
  } catch {
    if (record) record.failed = true;
    if (!res.headersSent) res.writeHead(502).end(); else res.destroy();
  }
});
await new Promise<void>((resolve,reject) => { relay.once('error',reject); relay.listen(0,'127.0.0.1',resolve); });
const address = relay.address();
if (!address || typeof address === 'string') throw new Error('Missing owned listener');
await mkdir(home,{recursive:true,mode:0o700});
const api = responses ? 'openai-responses' : 'openai-completions';
await writeFile(join(home,'models.json'),JSON.stringify({providers:{morphiecore:{baseUrl:`http://127.0.0.1:${address.port}/v1`,apiKey:'${MORPHIECORE_CLIENT_KEY}',api,
  compat:{supportsDeveloperRole:false,supportsStore:false,supportsStrictMode:true,maxTokensField:'max_completion_tokens',
    supportsOpenAIGrammarTools:false,supportsReasoningEffort:true,supportsUsageInStreaming:true,supportsLongCacheRetention:false},models:[{
  id:modelId,name:modelId,api,reasoning:true,input:['text'],contextWindow:32768,maxTokens:tokens,
  cost:{input:0,output:0,cacheRead:0,cacheWrite:0},
  ...(responses ? {thinkingLevelMap:{off:'none'}} : {}),
}]}}}),{mode:0o600});
const runtime = await ModelRuntime.create({authPath:join(home,'auth.json'),modelsPath:join(home,'models.json')});
if (invalidAuth) await runtime.setRuntimeApiKey('morphiecore','synthetic-wrong-client-token');
const model = runtime.getModel('morphiecore',modelId);
if (!model) throw new Error('Unregistered model');
const resources: ResourceLoader = {
  getExtensions:() => ({extensions:[],errors:[],runtime:createExtensionRuntime()}),getSkills:() => ({skills:[],diagnostics:[]}),
  getPrompts:() => ({prompts:[],diagnostics:[]}),getThemes:() => ({themes:[],diagnostics:[]}),getAgentsFiles:() => ({agentsFiles:[]}),
  getSystemPrompt:() => 'Follow the synthetic test instructions exactly. Be concise.',getSystemPromptSource:() => undefined,
  getAppendSystemPrompt:() => [],getAppendSystemPromptSources:() => [],extendResources:() => {},reload:async () => {},
};
try {
  for (const tool of [false,true]) {
    caseName = tool ? 'read' : 'text';
    const read = defineTool({name:'read',label:'Read synthetic fixture',description:'Read the synthetic fixture.txt.',
      parameters:Type.Object({path:Type.String()}, {additionalProperties:false}),
      execute:async (_id,args) => {
        if (args.path !== 'fixture.txt' || reads !== 0) throw new Error('Only one synthetic read');
        reads++;
        return {content:[{type:'text',text:'value=42\n'}],details:{synthetic:true}};
      }});
    const {session} = await createAgentSession({cwd:home,agentDir:home,model,modelRuntime:runtime,thinkingLevel:thinking,
      resourceLoader:resources,sessionManager:SessionManager.inMemory(home),tools:tool ? ['read'] : [],customTools:tool ? [read] : [],
      settingsManager:SettingsManager.inMemory({compaction:{enabled:false},cacheWarming:'off',retry:{enabled:false,provider:{maxRetries:0,timeoutMs:130000}},
        enableAnalytics:false,enableInstallTelemetry:false,transport:'sse'})});
    try {
      await session.prompt(tool ? 'Call read exactly once with path fixture.txt. Then reply with only the numeric value in that result, without any other words or punctuation.' : 'Reply with exactly pong.');
      const assistant = [...session.messages].reverse().find(m => m.role === 'assistant');
      const text = assistant?.content?.filter(p => p.type === 'text').map(p => p.text).join('');
      const record = records.at(-1), expected = tool ? '42' : 'pong';
      const facts = record?.observation?.facts(expected,text);
      const ok = checking ? record?.http === (invalidAuth ? 401 : 502) : assistant?.stopReason === 'stop' && text?.trim() === expected &&
        facts?.eof === true && facts.consumer_matches_wire === true && (!tool || reads === 1 && record?.tool_results === 1);
      reports.push({case:caseName,ok:!!ok});
      if (!ok) break;
    } finally {session.dispose();}
  }
  const ok = reports.length === 2 && reports.every(r => r.ok) && records.length === (checking ? 2 : 3);
  if (!ok) process.exitCode = 1;
} finally {
  relay.closeAllConnections();await new Promise<void>((resolve,reject) => relay.close(error => error ? reject(error) : resolve()));
  for (const record of records) {
    const facts = record.observation?.facts(record.case === 'text' ? 'pong' : '42');
    const result = reports.find(report => report.case === record.case);
    const state = record.failed || !checking && record.http !== 200 ? 'failed' : result?.ok ? 'passed' : 'oracle_failed';
    control('finish',{attempt:record.attempt,state,metrics:{http:record.http ?? null,wire_closed:facts?.eof === true,
      content_ok:result?.ok === true,sdk_consumed:!checking && result?.ok === true}});
    console.log(JSON.stringify({attempt:record.attempt,state,http:record.http,wire_closed:facts?.eof === true}));
  }
}
