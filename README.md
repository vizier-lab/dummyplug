# dummyplug

An independent, keyless OpenAI Chat compatible test model for Vizier and other clients.

Run with `cargo run`. It listens on `127.0.0.1:9998` by default; set `DUMMYPLUG_BIND` to change the listening address. Configure Vizier with `DUMMYPLUG_URL=http://127.0.0.1:9998/v1` (its default) and select provider and model `dummyplug`.

## Public API

- `GET /healthz`
- `GET /v1/models`
- `POST /v1/chat/completions`, with whole JSON or SSE responses

The chat API needs no authorization. Its normal behavior preserves the Vizier dummyplug commands: `tools`, an exact tool name, a JSON object `{"tool":"name","arguments":{}}`, `context`, tool result echo, and lorem for other text. The default response reports zero usage. A model name supplied by the client is returned unchanged.

## Test controls

Request headers: `X-Dummyplug-Session`, `X-Dummyplug-Seed`, `X-Dummyplug-Delay-Ms`, `X-Dummyplug-Chunk-Delay-Ms`, `X-Dummyplug-Usage` (JSON), and `X-Dummyplug-Respect-Tool-Choice: true`. Without the last header, a legacy JSON tool command can call a tool even with `tool_choice: none`.

In a user message, use `DUMMYPLUG REPLY text`, `DUMMYPLUG DELAY milliseconds`, `DUMMYPLUG HOLD`, `DUMMYPLUG FAIL status`, `DUMMYPLUG USAGE json`, `DUMMYPLUG REASONING text`, `DUMMYPLUG TOOLS json-array`, or `DUMMYPLUG FINISH reason`. Holds require a session ID.

Session controls:

- `PUT /_dummyplug/scenarios/{session_id}` with `{"steps":[...]}`. A step can set `reply`, `tool_calls`, `failures`, `status`, `retry_after`, `delay_ms`, `hold`, `usage`, `reasoning`, `finish_reason`, or `truncate_stream`.
- `GET /_dummyplug/requests?session_id=...` reads recent requests without authorization headers.
- `POST /_dummyplug/release/{session_id}` releases held requests.
- `POST /_dummyplug/reset` with `{"session_id":"..."}` clears one session; with `{}` clears all.

Control endpoints accept loopback callers. For remote access, set `DUMMYPLUG_CONTROL_TOKEN` and send it in `X-Dummyplug-Control-Token`. Sessions expire after one hour of inactivity and are limited to 128; each retains at most 100 request records and 256 KiB of records.
