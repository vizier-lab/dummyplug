import { after, before } from 'node:test';
import { createAppServer } from '../src/server.mjs';

export function useServer() {
  let server;
  let baseUrl;
  before(async () => {
    server = createAppServer();
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, '127.0.0.1', resolve);
    });
    baseUrl = `http://127.0.0.1:${server.address().port}`;
  });
  after(async () => {
    if (server?.listening) await new Promise((resolve) => server.close(resolve));
  });
  return { get baseUrl() { return baseUrl; } };
}

export const functionTool = (name = 'echo', parameters = { type: 'object', properties: { text: { type: 'string', description: 'Text to echo' } }, required: ['text'] }) => ({
  type: 'function',
  function: { name, description: 'Echo a value', parameters },
});

export async function chat(baseUrl, body, headers = {}) {
  return fetch(`${baseUrl}/v1/chat/completions`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...headers },
    body: JSON.stringify(body),
  });
}
