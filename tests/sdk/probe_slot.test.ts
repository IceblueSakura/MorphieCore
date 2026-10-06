import test from 'node:test';
import assert from 'node:assert/strict';
import { ProbeSlots } from '../../examples/probe_slot.ts';

test('overlapping handlers cannot reserve the same asynchronous body-read budget', async () => {
  const slots = new ProbeSlots(3);
  const { promise: barrier, resolve: release } = Promise.withResolvers<void>();
  async function handler() {
    const id = slots.take();
    await barrier;
    return id;
  }
  const pending = Array.from({length:8},handler);
  release();
  const outcomes = await Promise.allSettled(pending);
  assert.deepEqual(outcomes.filter(item => item.status === 'fulfilled').map(item => item.value),[1,2,3]);
  assert.equal(slots.used,3);
});
