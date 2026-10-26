import assert from 'node:assert/strict';
import test from 'node:test';
import { chat, functionTool, useServer } from './helpers.mjs';

const server = useServer();

test('health and model discovery expose the dummyplug model', async () => {
  const health = await fetch(`${server.baseUrl}/healthz`);
  assert.equal(health.status, 200);
  assert.deepEqual(await health.json(), { status: 'ok' });
  const models = await (await fetch(`${server.baseUrl}/v1/models`)).json();
  assert.equal(models.data[0].id, 'dummyplug');
});

test('whole response has a valid OpenAI-compatible envelope without credentials', async () => {
  const response = await chat(server.baseUrl, { model: 'any-model-alias', messages: [{ role: 'user', content: 'tools' }] }, { authorization: 'Bearer arbitrary' });
  assert.equal(response.status, 200);
  const result = await response.json();
  assert.equal(result.object, 'chat.completion');
  assert.equal(result.model, 'any-model-alias');
  assert.equal(result.choices[0].finish_reason, 'stop');
  assert.equal(result.choices[0].message.content, 'This agent has no tools available.');
  assert.deepEqual(result.usage, { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 });
  assert.match(result.id, /^chatcmpl-dummyplug-/u);
});

test('tool JSON returns a tool call using JSON-string arguments', async () => {
  const response = await chat(server.baseUrl, {
    model: 'dummyplug', tool_choice: 'none', tools: [functionTool()],
    messages: [{ role: 'user', content: '{"tool":"echo","arguments":{"text":"ok"}}' }],
  });
  const result = await response.json();
  assert.equal(result.choices[0].finish_reason, 'tool_calls');
  assert.equal(result.choices[0].message.tool_calls[0].function.name, 'echo');
  assert.deepEqual(JSON.parse(result.choices[0].message.tool_calls[0].function.arguments), { text: 'ok' });
});

test('invalid payloads return a structured client error and unknown routes are absent', async () => {
  const invalid = await chat(server.baseUrl, { messages: [] });
  assert.equal(invalid.status, 400);
  assert.equal((await invalid.json()).error.type, 'invalid_request_error');
  const control = await fetch(`${server.baseUrl}/_dummyplug/requests`);
  assert.equal(control.status, 404);
});
