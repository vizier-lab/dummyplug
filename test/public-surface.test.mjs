import assert from 'node:assert/strict';
import test from 'node:test';
import { chat, useServer } from './helpers.mjs';

const server = useServer();

test('test-control routes are not exposed and control headers do not change legacy replies', async () => {
  for (const [method, path, body] of [
    ['PUT', '/_dummyplug/scenarios/session', { steps: [] }],
    ['GET', '/_dummyplug/requests?session_id=session'],
    ['POST', '/_dummyplug/reset', {}],
    ['POST', '/_dummyplug/release/session', {}],
  ]) {
    const response = await fetch(`${server.baseUrl}${path}`, { method, headers: { 'content-type': 'application/json' }, body: body ? JSON.stringify(body) : undefined });
    assert.equal(response.status, 404);
  }
  const plain = await (await chat(server.baseUrl, { model: 'dummyplug', messages: [{ role: 'user', content: 'tools' }] })).json();
  const withOldControls = await (await chat(server.baseUrl, { model: 'dummyplug', messages: [{ role: 'user', content: 'tools' }] }, {
    'x-dummyplug-seed': '1', 'x-dummyplug-usage': '{"prompt_tokens":12}',
  })).json();
  assert.equal(plain.choices[0].message.content, withOldControls.choices[0].message.content);
  assert.deepEqual(plain.usage, withOldControls.usage);
});
