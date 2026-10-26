import { randomUUID } from 'node:crypto';

const TOOL_REQUEST_SHAPE = 'Expected: `{"tool": "<tool name>", "arguments": { ... }}`';
const LOREM_WORDS = `lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat duis aute irure in reprehenderit voluptate velit esse cillum fugiat nulla pariatur excepteur sint occaecat cupidatat non proident sunt culpa qui officia deserunt mollit anim id est laborum`.split(' ');

function textContent(content) {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content.map((part) => {
    if (typeof part?.text === 'string') return part.text;
    if (part?.type === 'image_url' || part?.type === 'image') return '[image]';
    return '';
  }).filter(Boolean).join('\n');
}

function validFrontmatter(raw) {
  const lines = raw.split(/\r?\n/);
  if (lines[0] !== '---') return false;
  const end = lines.indexOf('---', 1);
  if (end < 0) return false;
  const yaml = lines.slice(1, end);
  if (yaml.length === 0) return false;
  // Vizier frontmatter is metadata YAML. This conservative syntax check accepts ordinary key/value,
  // list, and nested mapping entries while leaving malformed blocks untouched.
  return yaml.every((line) => !line.trim() || line.trimStart().startsWith('#') || /^\s*(?:[-?]|[\w.'"-]+\s*:)/u.test(line));
}

export function commandText(message) {
  if (message?.role !== 'user') return '';
  const content = message.content;
  let text;
  if (typeof content === 'string') text = content;
  else if (Array.isArray(content)) {
    text = content.filter((part) => typeof part?.text === 'string' && !part.text.startsWith('# Context\n'))
      .map((part) => part.text).join('\n');
  } else text = '';

  if (validFrontmatter(text)) {
    const lines = text.split(/\r?\n/);
    const end = lines.indexOf('---', 1);
    text = lines.slice(end + 1).join('\n');
  }
  text = text.split('\n\n# Attached Files\n', 1)[0].trim();
  if (text.startsWith('```') && text.endsWith('```') && text.length >= 6) {
    const inner = text.slice(3, -3);
    const newline = inner.indexOf('\n');
    text = newline < 0 ? inner : inner.slice(newline + 1);
  }
  return text.trim();
}

function resolveSchema(node, root, depth = 0) {
  if (!node || typeof node !== 'object' || depth > 8) return node ?? {};
  if (typeof node.$ref === 'string' && node.$ref.startsWith('#/')) {
    const target = node.$ref.slice(2).split('/').map((key) => key.replaceAll('~1', '/').replaceAll('~0', '~'))
      .reduce((value, key) => value?.[key], root);
    if (target && target !== node) return resolveSchema(target, root, depth + 1);
  }
  return node;
}

function schemaProperties(schema) {
  const root = resolveSchema(schema, schema);
  const properties = { ...(root.properties ?? {}) };
  const required = [...(Array.isArray(root.required) ? root.required : [])];
  for (const branch of root.allOf ?? []) {
    const resolved = resolveSchema(branch, schema);
    Object.assign(properties, resolved.properties ?? {});
    for (const name of resolved.required ?? []) if (!required.includes(name)) required.push(name);
  }
  const orderedRequired = required.filter((name) => Object.hasOwn(properties, name));
  const optional = Object.keys(properties).filter((name) => !required.includes(name)).sort();
  return [...orderedRequired, ...optional].map((name) => ({
    name,
    required: required.includes(name),
    schema: resolveSchema(properties[name], schema),
    description: properties[name]?.description ?? resolveSchema(properties[name], schema)?.description,
  }));
}

function sampleValue(schema, root, depth = 0) {
  if (!schema || typeof schema !== 'object' || depth > 8) return null;
  for (const key of ['default', 'examples', 'example']) {
    if (key === 'examples' && Array.isArray(schema.examples) && schema.examples.length) return schema.examples[0];
    if (key !== 'examples' && Object.hasOwn(schema, key)) return schema[key];
  }
  if (Object.hasOwn(schema, 'const')) return schema.const;
  if (Array.isArray(schema.enum) && schema.enum.length) return schema.enum[0];
  const resolved = resolveSchema(schema, root);
  if (resolved !== schema) return sampleValue(resolved, root, depth + 1);
  const branches = schema.anyOf ?? schema.oneOf;
  if (Array.isArray(branches)) {
    const branch = branches.find((item) => resolveSchema(item, root)?.type !== 'null');
    return branch ? sampleValue(branch, root, depth + 1) : null;
  }
  const type = schema.type;
  if (Array.isArray(type)) return sampleValue({ ...schema, type: type.find((item) => item !== 'null') ?? 'null' }, root, depth + 1);
  if (schema.allOf || type === 'object' || schema.properties) {
    const props = {};
    for (const { name, schema: prop } of schemaProperties(schema)) props[name] = sampleValue(prop, root, depth + 1);
    return props;
  }
  if (type === 'array') return [];
  if (type === 'string') return schema.format === 'date-time' ? '2025-01-01T00:00:00Z' : 'string';
  if (type === 'integer' || type === 'number') return 0;
  if (type === 'boolean') return false;
  return null;
}

function toolList(tools) {
  if (!tools.length) return 'This agent has no tools available.';
  const lines = tools.map(({ name, description }) => {
    const first = String(description ?? '').trim().split(/\r?\n/, 1)[0].trim();
    const summary = first.length > 120 ? `${first.slice(0, 120)}…` : first;
    return summary ? `- \`${name}\` — ${summary}` : `- \`${name}\``;
  });
  return `**Available tools** (${tools.length}):\n\n${lines.join('\n')}\n\nSend a tool name to get a sample request.`;
}

function toolSample(tool) {
  const { name, description = '', parameters = {} } = tool;
  let reply = description.trim() ? `**\`${name}\`** — ${description.trim()}` : `**\`${name}\`**`;
  const props = schemaProperties(parameters);
  for (const [heading, required] of [['Required:', true], ['Optional:', false]]) {
    const group = props.filter((property) => property.required === required);
    if (!group.length) continue;
    reply += `\n\n${heading}\n${group.map(({ name: key, description: desc }) => `- \`${key}\`${desc ? `: ${String(desc).trim().replace(/\s+/gu, ' ')}` : ''}`).join('\n')}`;
  }
  const sample = sampleValue(parameters, parameters);
  const args = sample && typeof sample === 'object' && !Array.isArray(sample) ? sample : {};
  const pretty = JSON.stringify(args, null, 2).replaceAll('\n', '\n  ');
  reply += `\n\n\`\`\`json\n{\n  "tool": ${JSON.stringify(name)},\n  "arguments": ${pretty}\n}\n\`\`\`\n\nSend the JSON back (edit the values first) to run the tool.`;
  return reply;
}

function lorem() {
  const random = (min, max) => Math.floor(Math.random() * (max - min + 1)) + min;
  const paragraphs = [];
  for (let p = 0; p < random(1, 3); p += 1) {
    const sentences = [];
    for (let s = 0; s < random(2, 5); s += 1) {
      const words = Array.from({ length: random(6, 14) }, () => LOREM_WORDS[random(0, LOREM_WORDS.length - 1)]);
      words[0] = words[0][0].toUpperCase() + words[0].slice(1);
      sentences.push(`${words.join(' ')}.`);
    }
    paragraphs.push(sentences.join(' '));
  }
  return paragraphs.join('\n\n');
}

function lastToolResults(messages) {
  let start = messages.length;
  while (start > 0 && messages[start - 1]?.role === 'tool') start -= 1;
  if (start === messages.length) return null;
  const previousAssistant = [...messages.slice(0, start)].reverse().find((message) => message?.role === 'assistant');
  const calls = previousAssistant?.tool_calls ?? [];
  return messages.slice(start).map((result) => {
    const id = result.tool_call_id ?? '';
    const name = calls.find((call) => call.id === id)?.function?.name ?? id;
    return `**Tool result** (\`${name}\`):\n${textContent(result.content)}`;
  }).join('\n\n');
}

function contextText(message) {
  if (!Array.isArray(message?.content)) return '(no context block)';
  return message.content.find((part) => typeof part?.text === 'string' && part.text.startsWith('# Context\n'))?.text ?? '(no context block)';
}

function parseToolCommand(text, tools) {
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    return { content: `Could not parse tool request: ${error.message}\n\n${TOOL_REQUEST_SHAPE}` };
  }
  const invalid = (reason) => ({ content: `Invalid tool request: ${reason}\n\n${TOOL_REQUEST_SHAPE}` });
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return invalid('expected a JSON object');
  if (!Object.hasOwn(parsed, 'tool')) return invalid('missing `tool`');
  if (typeof parsed.tool !== 'string') return invalid('`tool` must be a string');
  if (Object.hasOwn(parsed, 'arguments') && (!parsed.arguments || typeof parsed.arguments !== 'object' || Array.isArray(parsed.arguments))) {
    return invalid('`arguments` must be an object');
  }
  if (!tools.some((tool) => tool.name === parsed.tool)) {
    return { content: `Unknown tool \`${parsed.tool}\`. Send \`tools\` to list available tools.` };
  }
  return { toolCall: { id: `dummyplug-${randomUUID()}`, type: 'function', function: {
    name: parsed.tool,
    arguments: JSON.stringify(parsed.arguments ?? {}),
  } } };
}

export function complete(messages, toolDefinitions) {
  const tools = (toolDefinitions ?? []).filter((tool) => tool?.type === 'function' && typeof tool.function?.name === 'string')
    .map((tool) => ({ name: tool.function.name, description: tool.function.description ?? '', parameters: tool.function.parameters ?? {} }));
  const results = lastToolResults(messages);
  if (results !== null) return { role: 'assistant', content: results };

  const userMessage = messages.at(-1);
  const text = commandText(userMessage);
  if (text.toLowerCase() === 'tools') return { role: 'assistant', content: toolList(tools) };
  if (text.toLowerCase() === 'context') return { role: 'assistant', content: contextText(userMessage) };
  const exactTool = tools.find((tool) => tool.name === text);
  if (exactTool) return { role: 'assistant', content: toolSample(exactTool) };
  if (text.startsWith('{')) {
    const parsed = parseToolCommand(text, tools);
    if (parsed.toolCall) return { role: 'assistant', content: null, tool_calls: [parsed.toolCall] };
    return { role: 'assistant', content: parsed.content };
  }
  return { role: 'assistant', content: lorem() };
}

export function completionResponse(request) {
  const message = complete(request.messages, request.tools);
  const hasToolCall = Array.isArray(message.tool_calls) && message.tool_calls.length > 0;
  return {
    id: `chatcmpl-dummyplug-${randomUUID()}`,
    object: 'chat.completion',
    created: Math.floor(Date.now() / 1000),
    model: request.model,
    choices: [{ index: 0, message, finish_reason: hasToolCall ? 'tool_calls' : 'stop' }],
    usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
  };
}

export function streamChunks(completion, includeUsage = false) {
  const choice = completion.choices[0];
  const wrap = (delta, finishReason = null) => ({
    id: completion.id,
    object: 'chat.completion.chunk',
    created: completion.created,
    model: completion.model,
    choices: [{ index: 0, delta, finish_reason: finishReason }],
  });
  const chunks = [wrap({ role: 'assistant' })];
  if (typeof choice.message.content === 'string') chunks.push(wrap({ content: choice.message.content }));
  for (const [index, call] of (choice.message.tool_calls ?? []).entries()) {
    chunks.push(wrap({ tool_calls: [{ index, id: call.id, type: call.type, function: call.function }] }));
  }
  chunks.push(wrap({}, choice.finish_reason));
  if (includeUsage) chunks.push({ id: completion.id, object: 'chat.completion.chunk', created: completion.created, model: completion.model, choices: [], usage: completion.usage });
  return chunks;
}
