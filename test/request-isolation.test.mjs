import assert from 'node:assert/strict';
import test from 'node:test';
import { complete } from '../src/protocol.mjs';

test('deterministic commands do not depend on earlier requests', () => {
  const ask = (value) => complete([{ role: 'user', content: value }], []);
  const first = ask('tools');
  ask('an unrelated request');
  ask('context');
  assert.deepEqual(ask('tools'), first);
});

test('unknown tool results fall back to the supplied call identifier', () => {
  const reply = complete([
    { role: 'assistant', tool_calls: [{ id: 'call-1', function: { name: 'not-this-call' } }] },
    { role: 'tool', tool_call_id: 'missing', content: 'result' },
  ], []);
  assert.equal(reply.content, '**Tool result** (`missing`):\nresult');
});
