# dummyplug

A standalone, stateless OpenAI Chat Completions test model for Vizier and other clients. It requires Node.js 22 or later and has no runtime dependencies.

## Clone and test

```sh
git clone --branch 030-node-dummyplug-server https://github.com/vizier-lab/dummyplug.git
cd dummyplug
npm ci
npm test
```

Requires Node.js 22 or later. `npm test` runs the server's Node.js test suite.

## Run locally

```sh
npm start
```

The server binds to `127.0.0.1:9998`. Check that it is running with:

```sh
curl http://127.0.0.1:9998/healthz
```

Set `DUMMYPLUG_HOST` or `DUMMYPLUG_PORT` to change the bind address or port.

## Try it with Vizier

Keep dummyplug running in one terminal. In a second terminal, start Vizier from its repository:

```sh
just run
```

Open `http://localhost:9999`, create the first account if needed, and create an agent with the
**dummyplug (testing)** provider and any model name. Vizier's default URL is
`http://127.0.0.1:9998/v1`, so no API key or extra provider configuration is needed. To use a
different address, set `DUMMYPLUG_URL` when starting Vizier or save a `base_url` under
Settings → Providers.

## Run in a container

```sh
docker build -t dummyplug .
docker run --rm -p 9998:9998 dummyplug
```

## Public API

- `GET /healthz`
- `GET /v1/models`
- `POST /v1/chat/completions`, returning JSON or server-sent events when `stream: true`

The server accepts any model string and does not require an API key. It preserves the legacy `tools`, `context`, exact tool name, JSON tool request, trailing tool-result echo, and lorem response behavior. Responses report zero usage. Scenario injection, request capture, and test-control routes belong in the caller's test harness; this service has no persistent request state.
