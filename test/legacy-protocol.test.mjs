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

test('say writes text in the same message as the tool call (spec 053)', () => {
  const message = complete([{ role: 'user', content: '{"say":"checking first","tool":"echo","arguments":{"text":"hi"}}' }], [functionTool()]);
  assert.equal(message.content, 'checking first');
  assert.equal(message.tool_calls.length, 1);
  assert.equal(message.tool_calls[0].function.name, 'echo');
});

test('an empty say is the same as none, and a non-string say is a text reply', () => {
  assert.equal(complete([{ role: 'user', content: '{"say":"","tool":"echo"}' }], [functionTool()]).content, null);
  assert.match(complete([{ role: 'user', content: '{"say":1,"tool":"echo"}' }], [functionTool()]).content, /^Invalid tool request: `say` must be a string/u);
});

test('pruned lists the pruned tool results the request carried, in order', () => {
  const call = (id) => ({ role: 'assistant', content: null, tool_calls: [{ id, type: 'function', function: { name: 'execute_python', arguments: '{}' } }] });
  const messages = [
    call('c1'), { role: 'tool', tool_call_id: 'c1', content: '[output pruned (74.3 KB) - use recall_history("h-1") to recall]' },
    call('c2'), { role: 'tool', tool_call_id: 'c2', content: '{"ok":true}' },
    call('c3'), { role: 'tool', tool_call_id: 'c3', content: [{ type: 'text', text: '[output pruned (1.2 MB) - use recall_history("h-3") to recall]' }] },
    { role: 'assistant', content: 'done' },
    { role: 'user', content: 'pruned' },
  ];
  assert.equal(complete(messages, []).content, '2 pruned:\n- c1 h-1 74.3 KB\n- c3 h-3 1.2 MB');
  assert.equal(complete([{ role: 'user', content: ' Pruned ' }], []).content, '(nothing pruned)');
});

test('sender echoes the frontmatter block of the newest user message', () => {
  const framed = '---\nmetadata:\n  platform_user_id: \'1\'\nsender: Unregistered User\nverified: false\n\n---\n\nsender';
  assert.equal(complete([{ role: 'user', content: [{ type: 'text', text: framed }] }], []).content,
    'metadata:\n  platform_user_id: \'1\'\nsender: Unregistered User\nverified: false');
  assert.equal(complete([{ role: 'user', content: 'sender' }], []).content, '(no frontmatter)');
});
