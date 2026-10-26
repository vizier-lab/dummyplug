"""Exercise the server through the stock OpenAI Python SDK."""
import os

from openai import OpenAI

client = OpenAI(
    api_key="dummy",
    base_url=os.environ.get("DUMMYPLUG_SDK_URL", "http://127.0.0.1:9998/v1"),
)
assert any(model.id == "dummyplug" for model in client.models.list().data)

messages = [{"role": "user", "content": "tools"}]
tool = {
    "type": "function",
    "function": {
        "name": "echo",
        "description": "Echo",
        "parameters": {"type": "object", "properties": {"text": {"type": "string"}}},
    },
}
whole = client.chat.completions.create(model="dummyplug", messages=messages, tools=[tool])
assert "echo" in whole.choices[0].message.content

streamed = client.chat.completions.create(model="dummyplug", messages=messages, tools=[tool], stream=True)
text = "".join(chunk.choices[0].delta.content or "" for chunk in streamed if chunk.choices)
assert text == whole.choices[0].message.content

command = [{"role": "user", "content": '{"tool":"echo","arguments":{"text":"hello"}}'}]
call = client.chat.completions.create(model="dummyplug", messages=command, tools=[tool])
assert call.choices[0].finish_reason == "tool_calls"
tool_call = call.choices[0].message.tool_calls[0]
assert tool_call.function.name == "echo"
result = client.chat.completions.create(
    model="dummyplug",
    messages=[
        command[0],
        call.choices[0].message,
        {"role": "tool", "tool_call_id": tool_call.id, "content": "hello"},
    ],
    tools=[tool],
)
assert result.choices[0].message.content == "**Tool result** (`echo`):\nhello"
print("dummyplug stock SDK conformance passed")
