import assert from 'node:assert/strict';
import test from 'node:test';
import { chat, functionTool, useServer } from './helpers.mjs';

const server = useServer();

async function parseSse(response) {
  const text = await response.text();
  const records = text.split('\n\n').map((part) => part.trim()).filter((part) => part.startsWith('data: ')).map((part) => part.slice(6));
  const done = records.pop();
  return { chunks: records.map((record) => JSON.parse(record)), done };
}

test('streamed content and finish reason reassemble to the whole completion', async () => {
  const body = { model: 'dummyplug', messages: [{ role: 'user', content: 'tools' }] };
  const whole = await (await chat(server.baseUrl, body)).json();
  const streamedResponse = await chat(server.baseUrl, { ...body, stream: true });
  assert.match(streamedResponse.headers.get('content-type'), /^text\/event-stream/u);
  const { chunks, done } = await parseSse(streamedResponse);
  assert.equal(chunks[0].choices[0].delta.role, 'assistant');
  const content = chunks.map((chunk) => chunk.choices[0].delta.content ?? '').join('');
  assert.equal(content, whole.choices[0].message.content);
  assert.equal(chunks.at(-1).choices[0].finish_reason, 'stop');
  assert.equal(done, '[DONE]');
});

test('tool-call streams include indexed deltas and optional usage', async () => {
  const response = await chat(server.baseUrl, {
    model: 'dummyplug', stream: true, stream_options: { include_usage: true }, tools: [functionTool()],
    messages: [{ role: 'user', content: '{"tool":"echo","arguments":{"text":"stream"}}' }],
  });
  const { chunks, done } = await parseSse(response);
  const toolChunk = chunks.find((chunk) => chunk.choices[0].delta.tool_calls);
  assert.equal(toolChunk.choices[0].delta.tool_calls[0].index, 0);
  assert.equal(toolChunk.choices[0].delta.tool_calls[0].function.name, 'echo');
  assert.deepEqual(JSON.parse(toolChunk.choices[0].delta.tool_calls[0].function.arguments), { text: 'stream' });
  assert.equal(chunks.find((chunk) => chunk.choices.length === 0).usage.total_tokens, 0);
  assert.equal(done, '[DONE]');
});
