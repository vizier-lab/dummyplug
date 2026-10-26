mod json_schema;
mod protocol;

use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post, put},
};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify};

const MAX_SESSIONS: usize = 128;
const MAX_RECORDS: usize = 100;
const MAX_BYTES: usize = 256 * 1024;
const TTL: Duration = Duration::from_secs(3600);

#[derive(Clone)]
struct App {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    token: Option<String>,
}
struct Session {
    touched: Instant,
    steps: VecDeque<Step>,
    records: VecDeque<Value>,
    bytes: usize,
    release: Arc<Notify>,
}
impl Session {
    fn new() -> Self {
        Self {
            touched: Instant::now(),
            steps: VecDeque::new(),
            records: VecDeque::new(),
            bytes: 0,
            release: Arc::new(Notify::new()),
        }
    }
}
#[derive(Clone, Deserialize, Serialize, Default)]
struct Step {
    #[serde(default)]
    reply: Option<String>,
    #[serde(default)]
    tool_calls: Vec<Value>,
    #[serde(default)]
    failures: u32,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    retry_after: Option<String>,
    #[serde(default)]
    delay_ms: Option<u64>,
    #[serde(default)]
    hold: bool,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    truncate_stream: bool,
}
#[derive(Clone, Deserialize, Serialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt_tokens_details: Option<Value>,
    #[serde(default, skip_serializing)]
    cached_tokens: Option<u64>,
}
impl Usage {
    fn normalized(mut self) -> Self {
        if self.total_tokens == 0 {
            self.total_tokens = self.prompt_tokens.saturating_add(self.completion_tokens);
        }
        if let Some(cached) = self.cached_tokens {
            self.prompt_tokens_details = Some(json!({"cached_tokens":cached}));
        }
        self
    }
}
#[derive(Deserialize, Serialize)]
struct Scenario {
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Reset {
    session_id: Option<String>,
}
#[derive(Deserialize)]
struct RequestsQuery {
    session_id: String,
}
#[derive(Deserialize)]
struct Chat {
    model: String,
    messages: Vec<Value>,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    tool_choice: Option<Value>,
    #[serde(default)]
    max_tokens: Option<u64>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    stream_options: Option<Value>,
}
#[derive(Default)]
struct Controls {
    session: Option<String>,
    seed: Option<u64>,
    delay_ms: u64,
    chunk_delay_ms: u64,
    usage: Option<Usage>,
    respect_tool_choice: bool,
}
#[derive(Debug)]
struct Error(StatusCode, String, Option<String>);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let mut response = (
            self.0,
            Json(json!({"error":{"message":self.1,"type":"invalid_request_error"}})),
        )
            .into_response();
        if let Some(value) = self.2.and_then(|v| HeaderValue::from_str(&v).ok()) {
            response.headers_mut().insert("retry-after", value);
        }
        response
    }
}
fn bad(msg: impl Into<String>) -> Error {
    Error(StatusCode::BAD_REQUEST, msg.into(), None)
}
fn header(headers: &HeaderMap, name: &str) -> Result<Option<String>, Error> {
    headers
        .get(name)
        .map(|v| {
            v.to_str()
                .map(str::to_string)
                .map_err(|_| bad(format!("invalid {name}")))
        })
        .transpose()
}
fn controls(headers: &HeaderMap) -> Result<Controls, Error> {
    let mut c = Controls {
        session: header(headers, "x-dummyplug-session")?,
        ..Default::default()
    };
    if c.session
        .as_ref()
        .is_some_and(|s| s.is_empty() || s.len() > 128)
    {
        return Err(bad("invalid session id"));
    }
    c.seed = header(headers, "x-dummyplug-seed")?
        .map(|v| v.parse().map_err(|_| bad("invalid seed")))
        .transpose()?;
    c.delay_ms = header(headers, "x-dummyplug-delay-ms")?
        .map(|v| v.parse().map_err(|_| bad("invalid delay")))
        .transpose()?
        .unwrap_or(0);
    c.chunk_delay_ms = header(headers, "x-dummyplug-chunk-delay-ms")?
        .map(|v| v.parse().map_err(|_| bad("invalid chunk delay")))
        .transpose()?
        .unwrap_or(0);
    if c.delay_ms > 300_000 || c.chunk_delay_ms > 300_000 {
        return Err(bad("delay exceeds 300000 ms"));
    }
    c.usage = header(headers, "x-dummyplug-usage")?
        .map(|v| serde_json::from_str(&v).map_err(|_| bad("invalid usage")))
        .transpose()?;
    c.respect_tool_choice = match header(headers, "x-dummyplug-respect-tool-choice")?.as_deref() {
        None | Some("false") => false,
        Some("true") => true,
        _ => return Err(bad("invalid respect-tool-choice")),
    };
    Ok(c)
}
fn authorized(app: &App, addr: SocketAddr, headers: &HeaderMap) -> Result<(), Error> {
    if addr.ip().is_loopback() {
        return Ok(());
    }
    let token = app.token.as_deref().ok_or_else(|| {
        Error(
            StatusCode::FORBIDDEN,
            "control API is local only".into(),
            None,
        )
    })?;
    if headers
        .get("x-dummyplug-control-token")
        .and_then(|h| h.to_str().ok())
        == Some(token)
    {
        Ok(())
    } else {
        Err(Error(
            StatusCode::UNAUTHORIZED,
            "invalid control token".into(),
            None,
        ))
    }
}
fn prune(sessions: &mut HashMap<String, Session>) {
    sessions.retain(|_, s| s.touched.elapsed() < TTL);
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}
async fn models() -> Json<Value> {
    Json(
        json!({"object":"list","data":[{"id":"dummyplug","object":"model","created":0,"owned_by":"dummyplug"}]}),
    )
}
async fn scenario(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Scenario>,
) -> Result<Json<Value>, Error> {
    authorized(&app, addr, &headers)?;
    if id.is_empty()
        || id.len() > 128
        || body.steps.len() > 100
        || serde_json::to_vec(&body).map_or(true, |b| b.len() > MAX_BYTES)
        || body.steps.iter().any(|step| {
            step.delay_ms.is_some_and(|delay| delay > 300_000)
                || step
                    .status
                    .is_some_and(|status| !(400..=599).contains(&status))
        })
    {
        return Err(bad("invalid scenario"));
    }
    let mut sessions = app.sessions.lock().await;
    prune(&mut sessions);
    if sessions.len() >= MAX_SESSIONS && !sessions.contains_key(&id) {
        return Err(bad("too many sessions"));
    }
    let session = sessions.entry(id).or_insert_with(Session::new);
    session.steps = body.steps.into();
    session.touched = Instant::now();
    Ok(Json(json!({"ok":true})))
}
async fn requests(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<RequestsQuery>,
) -> Result<Json<Value>, Error> {
    authorized(&app, addr, &headers)?;
    let sessions = app.sessions.lock().await;
    let session = sessions
        .get(&query.session_id)
        .ok_or_else(|| Error(StatusCode::NOT_FOUND, "session not found".into(), None))?;
    Ok(Json(json!({"requests":session.records})))
}
async fn reset(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Reset>,
) -> Result<Json<Value>, Error> {
    authorized(&app, addr, &headers)?;
    let mut sessions = app.sessions.lock().await;
    if let Some(id) = body.session_id {
        sessions.remove(&id);
    } else {
        sessions.clear();
    }
    Ok(Json(json!({"ok":true})))
}
async fn release(
    State(app): State<App>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    authorized(&app, addr, &headers)?;
    let sessions = app.sessions.lock().await;
    let session = sessions
        .get(&id)
        .ok_or_else(|| Error(StatusCode::NOT_FOUND, "session not found".into(), None))?;
    session.release.notify_waiters();
    Ok(Json(json!({"ok":true})))
}
async fn chat(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<Chat>,
) -> Result<Response, Error> {
    if req.model.is_empty() {
        return Err(bad("model must not be empty"));
    }
    let c = controls(&headers)?;
    let mut step = Step::default();
    let mut release = None;
    if let Some(id) = &c.session {
        let mut sessions = app.sessions.lock().await;
        prune(&mut sessions);
        if sessions.len() >= MAX_SESSIONS && !sessions.contains_key(id) {
            return Err(bad("too many sessions"));
        }
        let session = sessions.entry(id.clone()).or_insert_with(Session::new);
        session.touched = Instant::now();
        let record = json!({"messages":req.messages,"tools":req.tools,"tool_choice":req.tool_choice,"max_tokens":req.max_tokens,
            "headers":{"x-dummyplug-session":id,"x-dummyplug-seed":c.seed,"x-dummyplug-delay-ms":c.delay_ms,
                "x-dummyplug-chunk-delay-ms":c.chunk_delay_ms,"x-dummyplug-respect-tool-choice":c.respect_tool_choice}});
        let size = record.to_string().len();
        if size <= MAX_BYTES {
            while session.records.len() >= MAX_RECORDS || session.bytes + size > MAX_BYTES {
                if let Some(old) = session.records.pop_front() {
                    session.bytes -= old.to_string().len();
                } else {
                    break;
                }
            }
            session.records.push_back(record);
            session.bytes += size;
        }
        if let Some(front) = session.steps.front_mut() {
            if front.failures > 0 {
                front.failures -= 1;
                let status = StatusCode::from_u16(front.status.unwrap_or(503))
                    .unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
                return Err(Error(
                    status,
                    "injected dummyplug failure".into(),
                    front.retry_after.clone(),
                ));
            }
            step = session.steps.pop_front().unwrap();
        }
        release = Some(session.release.clone());
    }
    let command_text = protocol::command_text(req.messages.last());
    let exact_tool = req.tools.iter().any(|tool| {
        tool.pointer("/function/name").and_then(Value::as_str) == Some(command_text.as_str())
    });
    if !exact_tool && let Some(command) = command_text.strip_prefix("DUMMYPLUG ") {
        step = manual(command)?;
    }
    let mut held = if step.hold {
        let notifier = release.ok_or_else(|| bad("hold requires X-Dummyplug-Session"))?;
        let mut notified = Box::pin(notifier.notified_owned());
        notified.as_mut().enable();
        Some(notified)
    } else {
        None
    };
    let delay = step.delay_ms.unwrap_or(c.delay_ms);
    if delay > 300_000 {
        return Err(bad("delay exceeds 300000 ms"));
    }
    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    if let Some(notified) = held.take() {
        notified.await;
    }
    let usage = step
        .usage
        .clone()
        .unwrap_or(c.usage.unwrap_or_default())
        .normalized();
    let reply = protocol::reply(
        &req.messages,
        &req.tools,
        &req.tool_choice,
        c.seed,
        c.respect_tool_choice,
        &step,
    )?;
    let id = format!("chatcmpl-dummyplug-{:016x}", protocol::id(c.seed));
    if req.stream {
        let chunks = chunks(
            &id,
            &req.model,
            &reply.0,
            &reply.1,
            &usage,
            req.stream_options
                .as_ref()
                .and_then(|v| v.get("include_usage"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            step.truncate_stream,
        );
        let delay = c.chunk_delay_ms;
        let stream = stream::unfold(
            (chunks.into_iter(), delay),
            |(mut iter, delay)| async move {
                let next = iter.next()?;
                if delay > 0 {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                Some((
                    Ok::<Event, std::convert::Infallible>(Event::default().data(next)),
                    (iter, delay),
                ))
            },
        );
        Ok(Sse::new(stream).into_response())
    } else {
        Ok(Json(
            json!({"id":id,"object":"chat.completion","created":0,"model":req.model,
            "choices":[{"index":0,"message":reply.0,"finish_reason":reply.1}],"usage":usage}),
        )
        .into_response())
    }
}
fn manual(command: &str) -> Result<Step, Error> {
    let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
    let mut step = Step::default();
    match name.to_ascii_uppercase().as_str() {
        "REPLY" => step.reply = Some(arg.into()),
        "DELAY" => {
            step.delay_ms = Some(
                arg.parse()
                    .map_err(|_| bad("DELAY requires milliseconds"))?,
            )
        }
        "HOLD" => step.hold = true,
        "USAGE" => {
            step.usage = Some(serde_json::from_str(arg).map_err(|_| bad("USAGE requires JSON"))?)
        }
        "REASONING" => step.reasoning = Some(arg.into()),
        "FINISH" => step.finish_reason = Some(arg.into()),
        "TOOLS" => {
            step.tool_calls =
                serde_json::from_str(arg).map_err(|_| bad("TOOLS requires a JSON array"))?
        }
        "FAIL" => {
            return Err(Error(
                StatusCode::from_u16(arg.parse().map_err(|_| bad("FAIL requires status"))?)
                    .map_err(|_| bad("invalid status"))?,
                "injected dummyplug failure".into(),
                None,
            ));
        }
        _ => return Err(bad("unknown DUMMYPLUG command")),
    }
    Ok(step)
}
fn chunks(
    id: &str,
    model: &str,
    message: &Value,
    finish: &str,
    usage: &Usage,
    include_usage: bool,
    truncated: bool,
) -> Vec<String> {
    let wrap = |delta: Value, reason: Value| {
        json!({"id":id,"object":"chat.completion.chunk","created":0,"model":model,
        "choices":[{"index":0,"delta":delta,"finish_reason":reason}]})
        .to_string()
    };
    let mut out = vec![wrap(json!({"role":"assistant"}), Value::Null)];
    if let Some(content) = message.get("content").and_then(Value::as_str) {
        out.push(wrap(json!({"content":content}), Value::Null));
    }
    if let Some(reasoning) = message.get("reasoning_content") {
        out.push(wrap(json!({"reasoning_content":reasoning}), Value::Null));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            out.push(wrap(json!({"tool_calls":[{"index":index,"id":call["id"],"type":"function","function":call["function"]}]}), Value::Null));
        }
    }
    if !truncated {
        out.push(wrap(json!({}), json!(finish)));
        if include_usage {
            out.push(json!({"id":id,"object":"chat.completion.chunk","created":0,"model":model,"choices":[],"usage":usage}).to_string());
        }
        out.push("[DONE]".into());
    }
    out
}
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let bind = std::env::var("DUMMYPLUG_BIND").unwrap_or_else(|_| "127.0.0.1:9998".into());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("bind dummyplug server");
    let app = App {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        token: std::env::var("DUMMYPLUG_CONTROL_TOKEN")
            .ok()
            .filter(|s| !s.is_empty()),
    };
    let router = Router::new()
        .route("/healthz", get(health))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/_dummyplug/scenarios/{session_id}", put(scenario))
        .route("/_dummyplug/requests", get(requests))
        .route("/_dummyplug/reset", post(reset))
        .route("/_dummyplug/release/{session_id}", post(release))
        .with_state(app);
    tracing::info!(%bind, "dummyplug listening");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("serve dummyplug");
}
