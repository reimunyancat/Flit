use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::head;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use qrcode::QrCode;
use qrcode::render::svg;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::format;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

#[derive(Clone, Serialize)]
struct Item {
    id: String,
    kind: String,
    name: String,
    size: usize,
    text: Option<String>,
    created: u64,
    expires: u64,
    #[serde(skip_serializing)]
    bytes: Vec<u8>,
    #[serde(skip_serializing)]
    content_type: String,
}

#[derive(Clone)]
struct Share {
    item_id: String,
    expires: u64,
    once: bool,
}

#[derive(Clone)]
struct Drop {
    label: String,
    expires: u64,
}

#[derive(Clone)]
struct AppState {
    items: Arc<Mutex<HashMap<String, Item>>>,
    tx: broadcast::Sender<String>,
    token: Arc<String>,
    ttl: u64,
    public_url: Arc<Mutex<Option<String>>>,
    shares: Arc<Mutex<HashMap<String, Share>>>,
    drops: Arc<Mutex<HashMap<String, Drop>>>,
    rate: Arc<Mutex<HashMap<String, (u64, u32)>>>,
    rate_limit: u32,
    last_active: Arc<Mutex<u64>>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_url(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && !t.chars().any(|c| c.is_whitespace())
        && (t.starts_with("http://") || t.starts_with("https://"))
}

fn store(state: &AppState, item: Item) {
    let id = item.id.clone();
    state.items.lock().unwrap().insert(id.clone(), item);
    let _ = state.tx.send(id);
}

fn spawn_reaper(state: AppState) {
    if state.ttl == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            let t = now();
            state
                .items
                .lock()
                .unwrap()
                .retain(|_, it| it.expires == 0 || it.expires > t);
        }
    });
}

async fn icon() -> Response {
    let svg = include_str!("../static/icon.svg");
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("image/svg+xml"),
    );
    (h, svg).into_response()
}

async fn post_text(State(state): State<AppState>, body: String) -> Response {
    if body.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "empty").into_response();
    }
    let kind = if is_url(&body) { "link" } else { "text" };
    let label: String = body.lines().next().unwrap_or("").chars().take(80).collect();
    let created = now();
    let item = Item {
        id: Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        name: if label.trim().is_empty() {
            "text".into()
        } else {
            label
        },
        size: body.len(),
        text: Some(body.clone()),
        created,
        expires: if state.ttl == 0 {
            0
        } else {
            created + state.ttl
        },
        bytes: body.into_bytes(),
        content_type: "text/plain; charset=utf-8".into(),
    };
    store(&state, item);
    (StatusCode::CREATED, "ok").into_response()
}

async fn list_items(State(state): State<AppState>) -> Response {
    let map = state.items.lock().unwrap();
    let mut items: Vec<Item> = map.values().cloned().collect();
    items.sort_by(|a, b| b.created.cmp(&a.created));
    Json(items).into_response()
}

async fn health() -> &'static str {
    "ok"
}

async fn post_file(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut count = 0;
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "file".into());
        let content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());
        let data = match field.bytes().await {
            Ok(b) => b,
            Err(_) => continue,
        };
        let created = now();
        let item = Item {
            id: Uuid::new_v4().to_string(),
            kind: "file".into(),
            name,
            size: data.len(),
            text: None,
            created,
            expires: if state.ttl == 0 {
                0
            } else {
                created + state.ttl
            },
            bytes: data.to_vec(),
            content_type,
        };
        store(&state, item);
        count += 1;
    }
    if count == 0 {
        return (StatusCode::BAD_REQUEST, "no file field").into_response();
    }
    (StatusCode::CREATED, "ok").into_response()
}

async fn delete_item(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let removed = state.items.lock().unwrap().remove(&id).is_some();
    if removed {
        let _ = state.tx.send(String::new());
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::NOT_FOUND, "not found").into_response()
    }
}

async fn clear_items(State(state): State<AppState>) -> Response {
    state.items.lock().unwrap().clear();
    let _ = state.tx.send(String::new());
    StatusCode::NO_CONTENT.into_response()
}

async fn get_raw(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let map = state.items.lock().unwrap();
    match map.get(&id) {
        Some(item) => {
            let mut h = HeaderMap::new();
            if let Ok(v) = item.content_type.parse() {
                h.insert(header::CONTENT_TYPE, v);
            }
            let safe = item.name.replace('"', "").replace('\n', " ");
            if let Ok(v) = format!("inline; file_name=\"{safe}\"").parse() {
                h.insert(header::CONTENT_DISPOSITION, v);
            }
            (h, item.bytes.clone()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn app_js() -> Response {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    (h, include_str!("../static/app.js")).into_response()
}

async fn styles() -> Response {
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/css; charset=utf-8"),
    );
    (h, include_str!("../static/style.css")).into_response()
}

async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.tx.subscribe();
    let initial = tokio_stream::once(Ok::<Event, std::convert::Infallible>(
        Event::default().event("ready").data("ok"),
    ));
    let stream = initial.chain(BroadcastStream::new(rx).filter_map(|msg| {
        msg.ok()
            .map(|id| Ok::<_, std::convert::Infallible>(Event::default().event("item").data(id)))
    }));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        if let Some(v) = part.trim().strip_prefix("flit_token=") {
            return Some(v.to_string());
        }
    }
    None
}

async fn auth(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Response {
    if state.token.is_empty() {
        return next.run(req).await;
    }
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());
    let legacy = headers
        .get("x-flit-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let candidate = bearer
        .or(legacy)
        .or(q.token)
        .or_else(|| cookie_token(&headers));
    if candidate.as_deref() == Some(state.token.as_str()) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }
}

fn host_base(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(u) = state.public_url.lock().unwrap().clone() {
        return u;
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:7777");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("http");
    format!("{proto}://{host}")
}

async fn info(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let public = state.public_url.lock().unwrap().clone();
    let base = host_base(&state, &headers);
    Json(serde_json::json!({ "url": base, "tunnel": public.is_some()})).into_response()
}

async fn pairing_qr(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let base = host_base(&state, &headers);
    let url = if state.token.is_empty() {
        format!("{base}/")
    } else {
        format!("{base}/?token={}", state.token)
    };
    match QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let image = code.render::<svg::Color>().min_dimensions(220, 220).build();
            let mut h = HeaderMap::new();
            h.insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("image/svg+xml"),
            );
            (h, image).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "qr error").into_response(),
    }
}

fn spawn_tunnel(state: AppState, port: u16) {
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;
        let mut child = match Command::new("cloudflared")
            .arg("tunnel")
            .arg("--url")
            .arg(format!("http://127.0.0.1:{port}"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => {
                eprintln!("flit: FLIT_TUNNEL set but `cloudflared` was not found in PATH");
                return;
            }
        };
        if let Some(err) = child.stderr.take() {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(idx) = line.find("https://") {
                    let candidate = line[idx..]
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_end_matches('|')
                        .to_string();
                    if candidate.contains("trycloudflare.com")
                        || candidate.contains("cfargotunnel.com")
                    {
                        println!("flit: public URL -> {candidate}");
                        *state.public_url.lock().unwrap() = Some(candidate);
                    }
                }
            }
        }
        let _ = child.wait().await;
    });
}

#[tokio::main]
async fn main() {
    let addr: SocketAddr = match std::env::var("PORT") {
        Ok(p) => format!("0.0.0.0:{p}")
            .parse()
            .expect("PORT must be a valid port number"),
        Err(_) => std::env::var("FLIT_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:7777".into())
            .parse()
            .expect("FLIT_ADDR must be host:port"),
    };
    let ttl: u64 = std::env::var("FLIT_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    let tunnel = std::env::var("FLIT_TUNNEL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let max_mb: usize = std::env::var("FLIT_MAX_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    let rate_limit: u32 = std::env::var("FLIT_RATE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let ephemeral = std::env::var("FLIT_EPHEMERAL")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let idle_secs: u64 = std::env::var("FLIT_IDLE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let token = std::env::var("FLIT_TOKEN").unwrap_or_default();
    let (tx, _rx) = broadcast::channel::<String>(256);
    let state = AppState {
        items: Arc::new(Mutex::new(HashMap::new())),
        tx,
        token: Arc::new(token),
        ttl,
        public_url: Arc::new(Mutex::new(None)),
        shares: Arc::new(Mutex::new(HashMap::new())),
        drops: Arc::new(Mutex::new(HashMap::new())),
        rate: Arc::new(Mutex::new(HashMap::new())),
        rate_limit,
        last_active: Arc::new(Mutex::new(now())),
    };
    spawn_reaper(state.clone());
    if tunnel {
        spawn_tunnel(state.clone(), addr.port());
    }

    let api = Router::new()
        .route("/api/text", post(post_text))
        .route("/api/file", post(post_file))
        .route("/api/items", get(list_items).delete(clear_items))
        .route("/api/items/{id}/raw", get(get_raw))
        .route("/api/items/{id}", delete(delete_item))
        .route("/api/events", get(events))
        .route("/qr", get(pairing_qr))
        .route("/api/info", get(info))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/app.js", get(app_js))
        .route("/style.css", get(styles))
        .route("/icon.svg", get(icon))
        .merge(api)
        .layer(DefaultBodyLimit::max(max_mb * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("flit server listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
