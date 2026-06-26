use axum::extract::multipart::{self, Field};
use axum::response::Html;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::serve::Listener;
use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Serialize, ser};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use uuid::{Uuid, uuid};

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
struct AppState {
    items: Arc<Mutex<HashMap<String, Item>>>,
    tx: broadcast::Sender<String>,
    ttl: u64,
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

async fn icon() -> Response {
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 512 512'><rect width='512' height='512' rx='112' fill='#2dd672'/><path d='M406 106 L106 211 L241 271 Z' fill='#bdf0d0'/><path d='M406 106 L241 271 L301 406 Z' fill='#ffffff'/></svg>";
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

async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| {
        msg.ok()
            .map(|id| Ok::<_, std::convert::Infallible>(Event::default().event("item").data(id)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[tokio::main]
async fn main() {
    let addr: SocketAddr = std::env::var("FLIT_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:7777".into())
        .parse()
        .expect("FLIT_ADDR must be host:port");
    let ttl: u64 = std::env::var("FLIT_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);
    let (tx, _rx) = broadcast::channel::<String>(256);
    let state = AppState {
        items: Arc::new(Mutex::new(HashMap::new())),
        tx,
        ttl,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/icon.svg", get(icon))
        .route("/api/events", get(events))
        .route("/api/text", post(post_text))
        .route("/api/file", post(post_file))
        .route("/api/items", get(list_items))
        .route("/api/items/{id}/raw", get(get_raw))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("flit server listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
