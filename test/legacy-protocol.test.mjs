import assert from 'node:assert/strict';
import test from 'node:test';
import { commandText, complete } from '../src/protocol.mjs';
import { functionTool } from './helpers.mjs';

test('tools command preserves tool ordering, descriptions, and empty-list wording', () => {
  assert.equal(complete([{ role: 'user', content: 'tools' }], []).content, 'This agent has no tools available.');
  assert.equal(complete([{ role: 'user', content: 'tools' }], [functionTool()]).content,
    '**Available tools** (1):\n\n- `echo` — Echo a value\n\nSend a tool name to get a sample request.');
});

test('an exact tool name describes arguments and a usable JSON sample', () => {
  const reply = complete([{ role: 'user', content: 'echo' }], [functionTool()]).content;
  assert.match(reply, /Required:\n- `text`: Text to echo/u);
  assert.match(reply, /"tool": "echo"/u);
  assert.match(reply, /"arguments": \{/u);
});

test('valid tool JSON becomes an actionable call even when tool_choice is none', () => {
  const message = complete([{ role: 'user', content: '{"tool":"echo","arguments":{"text":"hi"}}' }], [functionTool()]);
  assert.equal(message.role, 'assistant');
  assert.equal(message.content, null);
  assert.equal(message.tool_calls.length, 1);
  assert.equal(message.tool_calls[0].function.name, 'echo');
  assert.deepEqual(JSON.parse(message.tool_calls[0].function.arguments), { text: 'hi' });
});

test('malformed, invalid, and unknown tool requests return assistant text', () => {
  assert.match(complete([{ role: 'user', content: '{"tool":' }], [functionTool()]).content, /^Could not parse tool request:/u);
  assert.match(complete([{ role: 'user', content: '{"tool":"echo","arguments":[]}' }], [functionTool()]).content, /^Invalid tool request: `arguments` must be an object/u);
  assert.equal(complete([{ role: 'user', content: '{"tool":"missing"}' }], [functionTool()]).content,
    'Unknown tool `missing`. Send `tools` to list available tools.');
});

test('frontmatter, attached-file trailer, fences, and separate context normalize as Vizier sends them', () => {
  const message = { role: 'user', content: [
    { type: 'text', text: '---\nuser: tester\n---\n```json\ntools\n```\n\n# Attached Files\n- file.png' },
    { type: 'text', text: '# Context\ncontext payload' },
  ] };
  assert.equal(commandText(message), 'tools');
  assert.equal(complete([message], []).content, 'This agent has no tools available.');
  assert.equal(complete([{ role: 'user', content: [{ type: 'text', text: 'context' }, { type: 'text', text: '# Context\ncontext payload' }] }], []).content,
    '# Context\ncontext payload');
});

test('tool results take precedence and resolve tool names across multiple trailing results', () => {
  const messages = [
    { role: 'assistant', tool_calls: [{ id: 'a', function: { name: 'first' } }, { id: 'b', function: { name: 'second' } }] },
    { role: 'tool', tool_call_id: 'a', content: 'one' },
    { role: 'tool', tool_call_id: 'unresolved', content: 'two' },
  ];
  assert.equal(complete(messages, []).content, '**Tool result** (`first`):\none\n\n**Tool result** (`unresolved`):\ntwo');
});

test('ordinary text has the documented lorem response shape', () => {
  const value = complete([{ role: 'user', content: 'ordinary prompt' }], []).content;
  const paragraphs = value.split('\n\n');
  assert.ok(paragraphs.length >= 1 && paragraphs.length <= 3);
  for (const paragraph of paragraphs) {
    const sentences = (paragraph.match(/[^.]+\./gu) ?? []).map((sentence) => sentence.trim());
    assert.ok(sentences.length >= 2 && sentences.length <= 5);
    for (const sentence of sentences) {
      const words = sentence.slice(0, -1).split(/\s+/u);
      assert.ok(words.length >= 6 && words.length <= 14);
      assert.match(sentence, /^[A-Z]/u);
    }
  }
});
