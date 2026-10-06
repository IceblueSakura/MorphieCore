// Pure synthetic diagnostics; no Pi runtime, credentials, files or network.
import test from 'node:test';
import assert from 'node:assert/strict';
import { WireObservation, classifyText } from '../../examples/provider_probe_observation.ts';
const event = (value: unknown) => `data: ${JSON.stringify(value)}\n\n`;
const completed = (text: string) => ({type:'response.completed',response:{output:[{type:'message',content:[{type:'output_text',text}]}]}});

test('same wire distinguishes consumer loss from a literal-output violation', () => {
  const observer = new WireObservation('responses');
  const wire = event({type:'response.output_text.delta',output_index:0,content_index:0,delta:'"pong"'}) + event(completed('"pong"'));
  for (const byte of Buffer.from(wire)) observer.push(Uint8Array.of(byte));
  observer.finish();
  assert.deepEqual(observer.facts('pong','"pong"'), {
    eof:true, terminal:'response.completed', wire_available:true, wire_chars:6,
    wire_class:'quoted', delta_snapshot_equal:true, consumer_matches_wire:true,
  });
  assert.equal(observer.facts('pong','pong').consumer_matches_wire,false);
  assert.equal(observer.facts('pong',' "pong" ').consumer_matches_wire,false);
  assert.equal(classifyText('Pong','pong'),'case_variant');
  assert.equal(classifyText('pong.','pong'),'punctuated');
  assert.equal(classifyText('pong','pong'),'exact');
});

test('UTF-8 fragments, part ordering and opaque reasoning cannot pollute answer facts', () => {
  const observer = new WireObservation('responses');
  const wire = event({type:'response.output_text.delta',output_index:1,content_index:0,delta:'界'})
    + event({type:'response.output_text.delta',output_index:0,content_index:0,delta:'世'})
    + event({type:'response.reasoning_text.delta',delta:'synthetic-private-thinking'})
    + event({type:'response.output_item.done',item:{type:'reasoning',encrypted_content:'synthetic-opaque-token'}})
    + event(completed('世界'));
  for (const byte of Buffer.from(wire)) observer.push(Uint8Array.of(byte));
  observer.finish();
  assert.equal(observer.facts('世界','世界').delta_snapshot_equal,true);
  assert.equal(observer.facts('世界','世界').wire_class,'exact');
  assert.ok(!JSON.stringify(observer.facts('世界','世界')).includes('synthetic'));
  assert.equal(JSON.stringify(observer),'{}');
});

test('CRLF accounting uses raw frame bytes and survives delimiter fragmentation', () => {
  const raw = new WireObservation('responses',{wireBytes:128,frameBytes:22,textChars:4});
  assert.throws(() => raw.push(Buffer.from(':c\r\n'.repeat(4) + 'data: {}\r\n\r\n')));
  const chat = new WireObservation('chat');
  const wire = (event({choices:[{index:0,delta:{content:'pong'},finish_reason:'stop'}]}) + 'data: [DONE]\n\n').replaceAll('\n','\r\n');
  for (const byte of Buffer.from(wire)) chat.push(Uint8Array.of(byte));
  chat.finish();
  assert.equal(chat.facts('pong','pong').consumer_matches_wire,true);
});

test('truncation, premature DONE and bounded allocation do not become success', () => {
  const observer = new WireObservation('chat');
  observer.push(Buffer.from(event({choices:[{index:0,delta:{content:'pong'},finish_reason:'stop'}]})));
  observer.finish();
  assert.equal(observer.facts('pong','pong').wire_available,false);
  const early = new WireObservation('chat');
  assert.throws(() => early.push(Buffer.from('data: [DONE]\n\n')));
  const limit = new WireObservation('responses',{wireBytes:16,frameBytes:8,textChars:4});
  assert.throws(() => limit.push(Buffer.from('data: '+ 'x'.repeat(20))));
  const partial = new WireObservation('responses');
  partial.push(Buffer.from('data: {"type":"response.completed"}'));
  assert.throws(() => partial.finish());
});
