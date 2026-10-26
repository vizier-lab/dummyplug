# dummyplug

A standalone, stateless OpenAI Chat Completions test model for Vizier and other clients. It requires Node.js 22 or later and has no runtime dependencies.

## Run locally

```sh
npm ci
npm test
npm start
```

The server binds to `127.0.0.1:9998`. Set `DUMMYPLUG_HOST` or `DUMMYPLUG_PORT` to change the bind address or port. Configure Vizier with `DUMMYPLUG_URL=http://127.0.0.1:9998/v1` (the default), then select provider `dummyplug` and any model string.

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
