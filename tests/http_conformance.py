"""Run against a separate dummyplug process: python3 tests/http_conformance.py."""
import json
import os
import urllib.error
import urllib.request

BASE = os.environ.get("DUMMYPLUG_HTTP_URL", "http://127.0.0.1:9998")


def call(path, body=None, headers=None, method=None):
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(
        BASE + path,
        data=data,
        headers={"Content-Type": "application/json", **(headers or {})},
        method=method,
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            raw = response.read().decode()
            return response.status, raw, dict(response.headers)
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode(), dict(error.headers)


def chat(text, *, tools=None, session=None, seed=42, stream=False, other=None):
    headers = {"X-Dummyplug-Seed": str(seed)}
    if session:
        headers["X-Dummyplug-Session"] = session
    body = {
        "model": "legacy-name",
        "messages": [{"role": "user", "content": text}],
        "tools": tools or [],
        "stream": stream,
        **(other or {}),
    }
    return call("/v1/chat/completions", body, headers)


assert json.loads(call("/healthz")[1])["status"] == "ok"
assert json.loads(call("/v1/models")[1])["data"][0]["id"] == "dummyplug"

status, raw, _ = chat("hello")
whole = json.loads(raw)
assert status == 200 and whole["model"] == "legacy-name"
assert whole["usage"] == {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
assert whole == json.loads(chat("hello")[1]), "seeded completion changed"
status, raw, _ = chat("hello", stream=True, other={"stream_options": {"include_usage": True}})
events = [line[6:] for line in raw.splitlines() if line.startswith("data: ")]
assert events[-1] == "[DONE]"
chunks = [json.loads(event) for event in events[:-1]]
assert chunks[0]["choices"][0]["delta"]["role"] == "assistant"
assert "".join(c["choices"][0]["delta"].get("content", "") for c in chunks if c["choices"]) == whole["choices"][0]["message"]["content"]
assert chunks[-1]["choices"] == [] and chunks[-1]["usage"] == whole["usage"]

tool = {"type": "function", "function": {"name": "echo", "description": "Echo", "parameters": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}}
assert "echo" in json.loads(chat("tools", tools=[tool])[1])["choices"][0]["message"]["content"]
assert '"tool": "echo"' in json.loads(chat("echo", tools=[tool])[1])["choices"][0]["message"]["content"]
completion = json.loads(chat('{"tool":"echo","arguments":{"text":"hello"}}', tools=[tool])[1])
message = completion["choices"][0]["message"]
assert completion["choices"][0]["finish_reason"] == "tool_calls"
assert message["tool_calls"][0]["function"]["name"] == "echo"
status, raw, _ = call("/v1/chat/completions", {
    "model": "dummyplug",
    "messages": [message, {"role": "tool", "tool_call_id": message["tool_calls"][0]["id"], "content": "hello"}],
})
assert status == 200 and json.loads(raw)["choices"][0]["message"]["content"] == "**Tool result** (`echo`):\nhello"

call("/_dummyplug/reset", {}, method="POST")
assert call("/_dummyplug/scenarios/A", {"steps": [{"reply": "done", "failures": 2, "status": 503, "retry_after": "1", "usage": {"prompt_tokens": 2, "completion_tokens": 3, "cached_tokens": 1}}]}, method="PUT")[0] == 200
assert call("/_dummyplug/scenarios/B", {"steps": [{"reply": "other"}]}, method="PUT")[0] == 200
for _ in range(2):
    status, _, headers = chat("hello", session="A")
    assert status == 503 and {k.lower(): v for k, v in headers.items()}["retry-after"] == "1"
assert json.loads(chat("hello", session="B")[1])["choices"][0]["message"]["content"] == "other"
answer = json.loads(chat("hello", session="A")[1])
assert answer["choices"][0]["message"]["content"] == "done"
assert answer["usage"]["total_tokens"] == 5
assert answer["usage"]["prompt_tokens_details"]["cached_tokens"] == 1
records = json.loads(call("/_dummyplug/requests?session_id=A")[1])["requests"]
assert len(records) == 3 and "Authorization" not in json.dumps(records)
assert call("/_dummyplug/reset", {"session_id": "A"}, method="POST")[0] == 200
assert call("/_dummyplug/requests?session_id=A")[0] == 404
print("dummyplug HTTP conformance passed")
