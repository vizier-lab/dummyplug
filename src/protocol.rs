use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde_json::{Value, json};

use crate::{Error, Step, bad, json_schema};

const SHAPE: &str = "Expected: `{\"tool\": \"<tool name>\", \"arguments\": { ... }}`";
const WORDS: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "in",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
];

pub fn id(seed: Option<u64>) -> u64 {
    match seed {
        Some(seed) => StdRng::seed_from_u64(seed).random(),
        None => rand::rng().random(),
    }
}

fn content_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn command_text(message: Option<&Value>) -> String {
    let Some(message) = message.filter(|m| m.get("role").and_then(Value::as_str) == Some("user"))
    else {
        return String::new();
    };
    let Some(content) = message.get("content") else {
        return String::new();
    };
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                p.get("text")
                    .and_then(Value::as_str)
                    .filter(|s| !s.starts_with("# Context\n"))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let body = frontmatter_body(&text).unwrap_or(text);
    let body = body
        .split_once("\n\n# Attached Files\n")
        .map(|(a, _)| a)
        .unwrap_or(&body)
        .trim();
    let body = if body.starts_with("```") && body.ends_with("```") && body.len() >= 6 {
        body[3..body.len() - 3]
            .split_once('\n')
            .map(|(_, inner)| inner)
            .unwrap_or(body)
    } else {
        body
    };
    body.trim().into()
}

/// Match Vizier's markdown frontmatter parser, including its CRLF behavior.
fn frontmatter_body(raw: &str) -> Option<String> {
    let mut lines = raw.split(['\n', '\r']);
    if lines.next() != Some("---") {
        return None;
    }
    let mut yaml = vec![];
    loop {
        match lines.next() {
            Some("---") => break,
            Some(line) => yaml.push(line),
            None => return None,
        }
    }
    serde_yaml::from_str::<serde_yaml::Value>(&yaml.join("\n")).ok()?;
    Some(lines.collect::<Vec<_>>().join("\n"))
}

fn tool_list(tools: &[(&str, &str, &Value)]) -> String {
    if tools.is_empty() {
        return "This agent has no tools available.".into();
    }
    let lines = tools
        .iter()
        .map(|(name, desc, _)| {
            let first = desc.trim().lines().next().unwrap_or("").trim();
            if first.is_empty() {
                format!("- `{name}`")
            } else {
                let mut summary = first.chars().take(120).collect::<String>();
                if first.chars().count() > 120 {
                    summary.push('…');
                }
                format!("- `{name}` — {summary}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "**Available tools** ({}):\n\n{lines}\n\nSend a tool name to get a sample request.",
        tools.len()
    )
}

fn tool_sample(name: &str, description: &str, schema: &Value) -> String {
    let mut reply = if description.trim().is_empty() {
        format!("**`{name}`**")
    } else {
        format!("**`{name}`** — {}", description.trim())
    };
    let properties = json_schema::properties(schema);
    let mut arguments = String::new();
    for (heading, required) in [("Required:", true), ("Optional:", false)] {
        let group = properties
            .iter()
            .filter(|p| p.required == required)
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        arguments.push_str(heading);
        for p in group {
            arguments.push_str(&format!(
                "\n- `{}`{}",
                p.name,
                p.description
                    .as_ref()
                    .map(|d| format!(": {d}"))
                    .unwrap_or_default()
            ));
        }
        arguments.push('\n');
    }
    if !arguments.is_empty() {
        reply.push_str("\n\n");
        reply.push_str(arguments.trim_end());
    }
    let sample = match json_schema::sample_value(schema) {
        v @ Value::Object(_) => v,
        _ => json!({}),
    };
    let sample = serde_json::to_string_pretty(&sample)
        .unwrap_or_else(|_| "{}".into())
        .replace('\n', "\n  ");
    reply.push_str(&format!("\n\n```json\n{{\n  \"tool\": {},\n  \"arguments\": {sample}\n}}\n```\n\nSend the JSON back (edit the values first) to run the tool.", json!(name)));
    reply
}

fn lorem(seed: Option<u64>) -> String {
    let mut rng = match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    (0..rng.random_range(1..=3))
        .map(|_| {
            (0..rng.random_range(2..=5))
                .map(|_| {
                    let mut sentence = (0..rng.random_range(6..=14))
                        .map(|_| WORDS[rng.random_range(0..WORDS.len())])
                        .collect::<Vec<_>>()
                        .join(" ");
                    sentence.get_mut(0..1).unwrap().make_ascii_uppercase();
                    sentence.push('.');
                    sentence
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn tool_result(messages: &[Value]) -> Option<String> {
    let mut start = messages.len();
    while start > 0 && messages[start - 1].get("role").and_then(Value::as_str) == Some("tool") {
        start -= 1;
    }
    if start == messages.len() {
        return None;
    }
    let calls = messages[..start]
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
        .and_then(|m| m.get("tool_calls"))
        .and_then(Value::as_array);
    Some(
        messages[start..]
            .iter()
            .map(|result| {
                let id = result
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let name = calls
                    .and_then(|cs| {
                        cs.iter()
                            .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
                    })
                    .and_then(|c| c.pointer("/function/name"))
                    .and_then(Value::as_str)
                    .unwrap_or(id);
                format!(
                    "**Tool result** (`{name}`):\n{}",
                    result.get("content").map(content_text).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

pub fn reply(
    messages: &[Value],
    definitions: &[Value],
    tool_choice: &Option<Value>,
    seed: Option<u64>,
    respect_choice: bool,
    step: &Step,
) -> Result<(Value, String), Error> {
    if !step.tool_calls.is_empty() {
        let calls = step.tool_calls.iter().enumerate().map(|(index, call)| {
            let name = call.get("name").and_then(Value::as_str).ok_or_else(|| bad("tool call name is required"))?;
            let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let arguments = if let Some(s) = arguments.as_str() { s.into() } else { arguments.to_string() };
            Ok(json!({"id":format!("dummyplug-{}",index+1),"type":"function","function":{"name":name,"arguments":arguments}}))
        }).collect::<Result<Vec<Value>, Error>>()?;
        return Ok((
            with_reasoning(
                json!({"role":"assistant","content":null,"tool_calls":calls}),
                step,
            ),
            finish(step, "tool_calls"),
        ));
    }
    if let Some(text) = &step.reply {
        return Ok((
            with_reasoning(json!({"role":"assistant","content":text}), step),
            finish(step, "stop"),
        ));
    }
    if let Some(text) = tool_result(messages) {
        return Ok((
            with_reasoning(json!({"role":"assistant","content":text}), step),
            finish(step, "stop"),
        ));
    }
    let text = command_text(messages.last());
    let tools: Vec<(&str, &str, &Value)> = definitions
        .iter()
        .filter_map(|t| {
            Some((
                t.pointer("/function/name")?.as_str()?,
                t.pointer("/function/description")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                t.pointer("/function/parameters")?,
            ))
        })
        .collect();
    let result = if text.eq_ignore_ascii_case("tools") {
        tool_list(&tools)
    } else if text.eq_ignore_ascii_case("context") {
        messages
            .last()
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
            .and_then(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .find(|s| s.starts_with("# Context\n"))
            })
            .unwrap_or("(no context block)")
            .into()
    } else if let Some((name, desc, schema)) = tools.iter().find(|(name, _, _)| *name == text) {
        tool_sample(name, desc, schema)
    } else if text.starts_with('{') {
        match serde_json::from_str::<Value>(&text) {
            Err(e) => format!("Could not parse tool request: {e}\n\n{SHAPE}"),
            Ok(value) => {
                let invalid = |why| format!("Invalid tool request: {why}\n\n{SHAPE}");
                let request = value.as_object();
                let outcome = match request {
                    None => Err(invalid("expected a JSON object")),
                    Some(obj) => match obj.get("tool") {
                        None => Err(invalid("missing `tool`")),
                        Some(Value::String(name)) => match obj.get("arguments") {
                            Some(args) if !args.is_object() => {
                                Err(invalid("`arguments` must be an object"))
                            }
                            _args if !tools.iter().any(|(n, _, _)| *n == name) => Err(format!(
                                "Unknown tool `{name}`. Send `tools` to list available tools."
                            )),
                            args => Ok((name.clone(), args.cloned().unwrap_or_else(|| json!({})))),
                        },
                        Some(_) => Err(invalid("`tool` must be a string")),
                    },
                };
                match outcome {
                    Ok((name, arguments))
                        if !(respect_choice
                            && tool_choice.as_ref().and_then(Value::as_str) == Some("none")) =>
                    {
                        let call = json!({"id":format!("dummyplug-{:016x}",id(seed)),"type":"function","function":{"name":name,"arguments":arguments.to_string()}});
                        return Ok((
                            with_reasoning(
                                json!({"role":"assistant","content":null,"tool_calls":[call]}),
                                step,
                            ),
                            finish(step, "tool_calls"),
                        ));
                    }
                    Ok(_) => "Tool calls are disabled by tool_choice: none.".into(),
                    Err(e) => e,
                }
            }
        }
    } else {
        lorem(seed)
    };
    Ok((
        with_reasoning(json!({"role":"assistant","content":result}), step),
        finish(step, "stop"),
    ))
}
fn with_reasoning(mut message: Value, step: &Step) -> Value {
    if let Some(reasoning) = &step.reasoning {
        message["reasoning_content"] = json!(reasoning);
    }
    message
}
fn finish(step: &Step, default: &str) -> String {
    step.finish_reason.clone().unwrap_or_else(|| default.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ask(text: &str) -> Vec<Value> {
        vec![json!({"role":"user","content":text})]
    }
    fn tool() -> Vec<Value> {
        vec![
            json!({"type":"function","function":{"name":"echo","description":"Echo a value","parameters":{"type":"object","properties":{"text":{"type":"string","description":"Text to echo"}},"required":["text"]}}}),
        ]
    }
    fn answer(messages: &[Value], tools: &[Value]) -> (Value, String) {
        reply(messages, tools, &None, Some(42), false, &Step::default()).unwrap()
    }
    #[test]
    fn legacy_commands_and_tool_round_trip() {
        assert_eq!(
            answer(&ask("tools"), &[]).0["content"],
            "This agent has no tools available."
        );
        assert_eq!(
            answer(&ask("tools"), &tool()).0["content"],
            "**Available tools** (1):\n\n- `echo` — Echo a value\n\nSend a tool name to get a sample request."
        );
        let sample = answer(&ask("echo"), &tool()).0["content"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(sample.contains("Required:\n- `text`: Text to echo"));
        assert!(sample.contains("\"tool\": \"echo\""));
        let (call, finish) = answer(
            &ask("{\"tool\":\"echo\",\"arguments\":{\"text\":\"hi\"}}"),
            &tool(),
        );
        assert_eq!(finish, "tool_calls");
        assert_eq!(call["tool_calls"][0]["function"]["name"], "echo");
        let id = call["tool_calls"][0]["id"].as_str().unwrap();
        let history = vec![
            json!({"role":"assistant","tool_calls":call["tool_calls"]}),
            json!({"role":"tool","tool_call_id":id,"content":"hi"}),
        ];
        assert_eq!(
            answer(&history, &tool()).0["content"],
            "**Tool result** (`echo`):\nhi"
        );
        assert_eq!(
            answer(&ask("{\"tool\":\"nope\"}"), &tool()).0["content"],
            "Unknown tool `nope`. Send `tools` to list available tools."
        );
        assert!(
            answer(&ask("{\"tool\":"), &tool()).0["content"]
                .as_str()
                .unwrap()
                .starts_with("Could not parse tool request:")
        );
    }
    #[test]
    fn context_frontmatter_and_seed() {
        let messages = vec![
            json!({"role":"user","content":[{"type":"text","text":"---\nuser: tester\n---\ncontext\n\n# Attached Files\nfile"},{"type":"text","text":"# Context\nhello"}]}),
        ];
        assert_eq!(answer(&messages, &[]).0["content"], "# Context\nhello");
        assert_eq!(answer(&ask("hello"), &[]).0, answer(&ask("hello"), &[]).0);
        assert_eq!(answer(&ask("HOLD"), &[]).1, "stop");
    }
    #[test]
    fn legacy_ignores_tool_choice_none_unless_enabled() {
        let messages = ask("{\"tool\":\"echo\"}");
        let choice = Some(json!("none"));
        assert_eq!(
            reply(
                &messages,
                &tool(),
                &choice,
                Some(1),
                false,
                &Step::default()
            )
            .unwrap()
            .1,
            "tool_calls"
        );
        assert_eq!(
            reply(&messages, &tool(), &choice, Some(1), true, &Step::default())
                .unwrap()
                .1,
            "stop"
        );
    }
    #[test]
    fn legacy_framing_and_precedence() {
        assert_eq!(
            command_text(Some(
                &json!({"role":"user","content":"---\nuser: tester\n---\n```json\n{\"tool\":\"echo\"}\n```"})
            )),
            "{\"tool\":\"echo\"}"
        );
        assert_eq!(
            command_text(Some(
                &json!({"role":"user","content":"---\ninvalid: [\n---\ntools"})
            )),
            "---\ninvalid: [\n---\ntools"
        );
        assert_eq!(
            command_text(Some(
                &json!({"role":"user","content":"tools\n\n# Attached Files\n- a.png"})
            )),
            "tools"
        );
        let shadow = vec![
            json!({"type":"function","function":{"name":"tools","description":"Shadow","parameters":{}}}),
        ];
        assert!(
            answer(&ask("tools"), &shadow).0["content"]
                .as_str()
                .unwrap()
                .starts_with("**Available tools**")
        );
    }
    #[test]
    fn multiple_trailing_results_keep_order_and_unknown_id() {
        let messages = vec![
            json!({"role":"assistant","tool_calls":[{"id":"known","function":{"name":"echo"}}]}),
            json!({"role":"tool","tool_call_id":"known","content":"one"}),
            json!({"role":"tool","tool_call_id":"missing","content":"two"}),
        ];
        assert_eq!(
            answer(&messages, &[]).0["content"],
            "**Tool result** (`echo`):\none\n\n**Tool result** (`missing`):\ntwo"
        );
    }
    #[test]
    fn sample_keeps_required_order_and_optional_descriptions() {
        let tool = vec![
            json!({"type":"function","function":{"name":"write_like","description":"Writes.","parameters":{
            "type":"object","properties":{"title":{"type":"string","description":"Title of the memory"},"content":{"type":"string"},"tags":{"type":"array","items":{"type":"string"},"description":"Tags used to group related memories"}},
            "required":["title","content"]}}}),
        ];
        let text = answer(&ask("write_like"), &tool).0["content"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(text.starts_with("**`write_like`** — Writes.\n\nRequired:\n- `title`: Title of the memory\n- `content`\nOptional:\n- `tags`: Tags used to group related memories\n\n```json"), "{text}");
        let json_text = text
            .split("```json\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap();
        let (call, finish) = answer(&ask(json_text), &tool);
        assert_eq!(finish, "tool_calls");
        assert_eq!(call["tool_calls"][0]["function"]["name"], "write_like");
    }
}
