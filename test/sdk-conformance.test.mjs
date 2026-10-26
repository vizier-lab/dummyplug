import assert from 'node:assert/strict';
import test from 'node:test';
import { useServer } from './helpers.mjs';

const server = useServer();

test('stock OpenAI JavaScript SDK can discover and call dummyplug when installed', async (context) => {
  let OpenAI;
  try {
    ({ default: OpenAI } = await import('openai'));
  } catch {
    context.skip('install the openai npm package to run SDK conformance');
    return;
  }
  const client = new OpenAI({ apiKey: 'not-required', baseURL: `${server.baseUrl}/v1` });
  const models = await client.models.list();
  assert.equal(models.data[0].id, 'dummyplug');
  const response = await client.chat.completions.create({ model: 'dummyplug', messages: [{ role: 'user', content: 'tools' }] });
  assert.equal(response.choices[0].message.content, 'This agent has no tools available.');
  const stream = await client.chat.completions.create({ model: 'dummyplug', stream: true, messages: [{ role: 'user', content: 'tools' }] });
  let content = '';
  for await (const chunk of stream) content += chunk.choices[0]?.delta?.content ?? '';
  assert.equal(content, response.choices[0].message.content);
});
