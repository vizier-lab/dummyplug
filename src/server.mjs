import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { completionResponse, streamChunks } from './protocol.mjs';

const MAX_BODY_BYTES = 2 * 1024 * 1024;

function json(response, statusCode, body) {
  response.writeHead(statusCode, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
    'x-content-type-options': 'nosniff',
  });
  response.end(JSON.stringify(body));
}

function apiError(response, statusCode, message) {
  json(response, statusCode, { error: { message, type: 'invalid_request_error' } });
}

async function readJson(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) throw Object.assign(new Error('request body exceeds 2 MiB'), { statusCode: 413 });
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  } catch {
    throw Object.assign(new Error('request body must be valid JSON'), { statusCode: 400 });
  }
}

function validateChat(body) {
  if (!body || typeof body !== 'object' || Array.isArray(body)) return 'request body must be a JSON object';
  if (typeof body.model !== 'string' || !body.model.trim()) return 'model must be a non-empty string';
  if (!Array.isArray(body.messages)) return 'messages must be an array';
  if (body.messages.some((message) => !message || typeof message !== 'object' || Array.isArray(message))) return 'each message must be an object';
  if (body.tools !== undefined && !Array.isArray(body.tools)) return 'tools must be an array';
  if (body.stream !== undefined && typeof body.stream !== 'boolean') return 'stream must be a boolean';
  if (body.stream_options !== undefined && (!body.stream_options || typeof body.stream_options !== 'object' || Array.isArray(body.stream_options))) return 'stream_options must be an object';
  return null;
}

async function writeSse(response, request, completion) {
  response.writeHead(200, {
    'content-type': 'text/event-stream; charset=utf-8',
    'cache-control': 'no-cache, no-transform',
    connection: 'keep-alive',
    'x-accel-buffering': 'no',
  });
  const includeUsage = request.stream_options?.include_usage === true;
  for (const chunk of streamChunks(completion, includeUsage)) {
    if (response.destroyed || response.writableEnded) return;
    if (!response.write(`data: ${JSON.stringify(chunk)}\n\n`)) {
      await new Promise((resolve) => {
        response.once('drain', resolve);
        response.once('close', resolve);
      });
      if (response.destroyed || response.writableEnded) return;
    }
  }
  if (!response.destroyed && !response.writableEnded) response.end('data: [DONE]\n\n');
}

export function createAppServer() {
  return createServer(async (request, response) => {
    const started = performance.now();
    let logged = false;
    const log = () => {
      if (logged) return;
      logged = true;
      process.stdout.write(`${JSON.stringify({ level: 'info', event: 'http_request', method: request.method, path: new URL(request.url ?? '/', 'http://localhost').pathname, status: response.statusCode, duration_ms: Math.round(performance.now() - started) })}\n`);
    };
    response.once('finish', log);
    response.once('close', log);
    const url = new URL(request.url ?? '/', 'http://localhost');
    if (request.method === 'GET' && url.pathname === '/healthz') {
      json(response, 200, { status: 'ok' });
      return;
    }
    if (request.method === 'GET' && url.pathname === '/v1/models') {
      json(response, 200, { object: 'list', data: [{ id: 'dummyplug', object: 'model', created: 0, owned_by: 'dummyplug' }] });
      return;
    }
    if (request.method !== 'POST' || url.pathname !== '/v1/chat/completions') {
      apiError(response, 404, 'not found');
      return;
    }

    let body;
    try {
      body = await readJson(request);
    } catch (error) {
      apiError(response, error.statusCode ?? 400, error.message);
      return;
    }
    const validationError = validateChat(body);
    if (validationError) {
      apiError(response, 400, validationError);
      return;
    }

    const completion = completionResponse(body);
    if (body.stream === true) {
      await writeSse(response, body, completion);
      return;
    }
    json(response, 200, completion);
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const host = process.env.DUMMYPLUG_HOST ?? '127.0.0.1';
  const port = Number(process.env.DUMMYPLUG_PORT ?? 9998);
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw new Error('DUMMYPLUG_PORT must be between 0 and 65535');
  const server = createAppServer();
  server.listen(port, host, () => {
    const address = server.address();
    process.stdout.write(`dummyplug listening on http://${host}:${address.port}\n`);
  });
  const shutdown = () => server.close(() => process.exit(0));
  process.once('SIGINT', shutdown);
  process.once('SIGTERM', shutdown);
}
