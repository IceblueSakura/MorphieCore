// Preserve the real client's auth boundary without any credential fixture.
import test from 'node:test';
import assert from 'node:assert/strict';
import { relayHeaders } from '../../examples/probe_http_headers.ts';

test('a relay neither repairs incorrect credentials nor fabricates missing headers', () => {
  assert.deepEqual(relayHeaders(['Authorization','Bearer synthetic-wrong','Content-Type','text/plain',
    'Proxy-Authorization','must-not-forward','X-Api-Key','must-not-forward','Host','other.invalid']),
    {authorization:'Bearer synthetic-wrong','content-type':'text/plain'});
  assert.deepEqual(relayHeaders([]),{});
});

test('duplicate selected headers fail closed independent of case', () => {
  for (const name of ['authorization','content-type']) {
    assert.throws(() => relayHeaders([name,'first',name.toUpperCase(),'second']));
  }
  assert.throws(() => relayHeaders(['Authorization']));
});
