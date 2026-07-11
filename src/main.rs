use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::head;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Deserialize)]
struct ShareReq {
    once: Option<bool>,
    ttl: Option<u64>,
}

async fn create_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ShareReq>,
) -> Response {
    if !state.items.lock().unwrap().contains_key(&id) {
        return (StatusCode::NOT_FOUND, "no item").into_response();
    }
    let sid = Uuid::new_v4().simple().to_string();
    let ttl = req.ttl.unwrap_or(0);
    let share = Share {
        item_id: id,
        expires: if ttl == 0 { 0 } else { now() + ttl },
        once: req.once.unwrap_or(false),
    };
    state.shares.lock().unwrap().insert(sid.clone(), share);
    let base = host_base(&state, &headers);
    Json(serde_json::json!({ "id":sid, "url": format!("{base}/s/{sid}")})).into_response()
}

async fn serve_share(State(state): State<AppState>, Path(sid): Path<String>) -> Response {
    let share = state.shares.lock().unwrap().get(&sid).cloned();
    let share = match share {
        Some(s) => s,
        None => return (StatusCode::NOT_FOUND, "expired or missing").into_response(),
    };
    if share.expires != 0 && share.expires < now() {
        state.shares.lock().unwrap().remove(&sid);
        return (StatusCode::NOT_FOUND, "expired").into_response();
    }
    let item = state.items.lock().unwrap().get(&share.item_id).cloned();
    let item = match item {
        Some(i) => i,
        None => return (StatusCode::NOT_FOUND, "item gone").into_response(),
    };
    if share.once {
        state.shares.lock().unwrap().remove(&sid);
    }
    if item.kind == "file" {
        let mut h = HeaderMap::new();
        if let Ok(v) = item.content_type.parse() {
            h.insert(header::CONTENT_TYPE, v);
        }
        let safe = item.name.replace('\n', " ");
        if let Ok(v) = format!("attachment; filename=\"{safe}\"").parse() {
            h.insert(header::CONTENT_DISPOSITION, v);
        }
        (h, item.bytes.clone()).into_response()
    } else {
        let esc = html_escape(&item.text.clone().unwrap_or_default());
        let mut p = String::from(
            "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>Flit share</title><body style='font:15px system-ui;max-width:680px;margin:40px auto;padding:0 16px'><h3><img src='/icon.svg' width='22' height='22' style='vertical-align:-4px;margin-right:6px'/>Flit</h3><pre style='white-space:pre-wrap;word-break:break-word;background:#80808022;padding:14px;border-radius:10px'>",
        );
        p.push_str(&esc);
        p.push_str("</pre></body>");
        Html(p).into_response()
    }
}

fn want_lang(headers: &HeaderMap) -> &'static str {
    let al = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let first = al
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if first.starts_with("ko") { "ko" } else { "en" }
}

#[derive(Deserialize)]
struct DropReq {
    label: Option<String>,
    ttl: Option<u64>,
}

async fn create_drop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DropReq>,
) -> Response {
    let did = Uuid::new_v4().simple().to_string();
    let ttl = req.ttl.unwrap_or(0);
    let drop = Drop {
        label: req.label.unwrap_or_else(|| "guest".into()),
        expires: if ttl == 0 { 0 } else { now() + ttl },
    };
    state.drops.lock().unwrap().insert(did.clone(), drop);
    let base = host_base(&state, &headers);
    Json(serde_json::json!({"id":did, "url": format!("{base}/d/{did}")})).into_response()
}

fn valid_drop(state: &AppState, did: &str) -> Option<Drop> {
    let d = state.drops.lock().unwrap().get(did).cloned()?;
    if d.expires != 0 && d.expires < now() {
        state.drops.lock().unwrap().remove(did);
        return None;
    }
    Some(d)
}

async fn drop_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(did): Path<String>,
) -> Response {
    match valid_drop(&state, &did) {
        Some(_) => {
            let (lang, title, note, ph, btn) = if want_lang(&headers) == "ko" {
                (
                    "ko",
                    "Flit 받기함",
                    "여기 올리면 상대방 인박스로만 전달됩니다. 인박스 내용은 안 보여요.",
                    "텍스트나 링크...",
                    "보내기",
                )
            } else {
                (
                    "en",
                    "Flit drop",
                    "Anything you send here lands only in their inbox - you can't see the inbox itself.",
                    "Text or a link...",
                    "send",
                )
            };
            let p = format!(
                "<!doctype html><html lang='{lang}'><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>Flit</title><body style='font:15px system-ui;max-width:560px;margin:40px auto;padding 0 16px'><h2><img src='/icon.svg' width='24' height='24' sytle='vertical-align:-5px;margin-right:6px'/>{title}</h2><p style=opacity:.6'>{note}</p><form method=post enctype='multipart/form-data'><textarea name=text placeholder='{ph}' style='width:100%;min-height:90px;padding:10px;border:1px solid #8888;border-radius:10px;background:transparent'></textarea><br><br><input type=file name=file><br><br><button style='padding:10px 16px;border:0,border-radius:9px;background:#2dd672;font-weight:600'>{btn}</button></form></body></html>"
            );
            Html(p).into_response()
        }
        None => (StatusCode::NOT_FOUND, "expired or missing").into_response(),
    }
}

async fn drop_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(did): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let drop = match valid_drop(&state, &did) {
        Some(d) => d,
        None => return (StatusCode::NOT_FOUND, "expired").into_response(),
    };
    let mut count = 0;
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().map(|s| s.to_string());
        let file_name = field
            .file_name()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let ctype = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());
        if let Some(name) = file_name {
            let data = match field.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            if data.is_empty() {
                continue;
            }
            let created = now();
            let item = Item {
                id: Uuid::new_v4().to_string(),
                kind: "file".into(),
                name: format!("[{}] {}", drop.label, name),
                size: data.len(),
                text: None,
                created,
                expires: if state.ttl == 0 {
                    0
                } else {
                    created + state.ttl
                },
                bytes: data.to_vec(),
                content_type: ctype,
            };
            store(&state, item);
            count += 1;
        } else if field_name.as_deref() == Some("text") {
            let data = match field.text().await {
                Ok(t) => t,
                Err(_) => continue,
            };
            if data.trim().is_empty() {
                continue;
            }
            let created = now();
            let kind = if is_url(&data) { "link" } else { "text" };
            let label: String = data.lines().next().unwrap_or("").chars().take(60).collect();
            let item = Item {
                id: Uuid::new_v4().to_string(),
                kind: kind.to_string(),
                name: format!("[{}] {}", drop.label, label),
                size: data.len(),
                text: Some(data.clone()),
                created,
                expires: if state.ttl == 0 {
                    0
                } else {
                    created + state.ttl
                },
                bytes: data.into_bytes(),
                content_type: "text/plain; charset=utf-8".into(),
            };
            store(&state, item);
            count += 1;
        } else {
            let _ = field.bytes().await;
        }
    }
    if count == 0 {
        return (StatusCode::BAD_REQUEST, "nothing uploaded").into_response();
    }
    let (msg, again) = if want_lang(&headers) == "ko" {
        ("전송 완료! 받는 사람 인박스에 도착했어요.", "또 보내기")
    } else {
        ("Sent! It's in their inbox now.", "Send another")
    };
    Html(format!("<!doctype html><meta charset=utf-8><body style='font:16px system-ui;text-align:center;margin-top:60px'>{msg}<br><br><a href=''>{again}</a></body>")).into_response()
}

async fn manifest() -> Response {
    let m = serde_json::json!({
        "name": "Flit",
        "short_name": "Flit",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "background_color": "#111111",
        "theme_color": "#2dd672",
        "icons": [{"src":"/icon.svg", "sizes":"any", "type":"image/svg+xml", "purpose": "any maskable"}],
        "share_target": {
            "action": "/share-target",
            "method": "POST",
            "enctype": "multipart/form-data",
            "params": {"title":"title","text": "text", "url":"url","files":[{"name":"file","accept":["*/*"]}]}
        }
    });
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/manifest+json"),
    );
    (h, m.to_string()).into_response()
}

async fn service_worker() -> Response {
    let js = r#"const C='flit-v2';const P=['/','/app.js','/style.css','/icon.svg'];self.addEventListener('install',e=>{self.skipWaiting();e.waitUntil(caches.open(C).then(c=>c.addAll(P)));});self.addEventListener('activate',e=>{e.waitUntil(caches.keys().then(ks=>Promise.all(ks.filter(k=>k!==C).map(k=>caches.delete(k)))).then(()=>self.clients.claim()));});self.addEventListener('fetch',e=>{if(e.request.method!=='GET')return;const u=new URL(e.request.url);if(P.includes(u.pathname)){e.respondWith(fetch(e.request).then(r=>{const cp=r.clone();caches.open(C).then(c=>c.put(e.request,cp));return r;}).catch(()=>caches.match(e.request)));}});"#;
    let mut h = HeaderMap::new();
    h.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/javascript"),
    );
    (h, js).into_response()
}

async fn share_target(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut texts: Vec<String> = Vec::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().map(|s| s.to_string());
        let file_name = field
            .file_name()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let ctype = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());
        if let Some(name) = field_name {
            let data = match field.bytes().await {
                Ok(b) => b,
                Err(_) => continue,
            };
            if data.is_empty() {
                continue;
            }
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
                content_type: ctype,
            };
            store(&state, item);
        } else if matches!(
            field_name.as_deref(),
            Some("text") | Some("url") | Some("title")
        ) {
            if let Ok(t) = field.text().await {
                if !t.trim().is_empty() {
                    texts.push(t);
                }
            }
        } else {
            let _ = field.bytes().await;
        }
    }
    let joined = texts.join(" ").trim().to_string();
    if !joined.is_empty() {
        let created = now();
        let kind = if is_url(&joined) { "link" } else { "text" };
        let label: String = joined
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        let item = Item {
            id: Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            name: if label.trim().is_empty() {
                "shared".into()
            } else {
                label
            },
            size: joined.len(),
            text: Some(joined.clone()),
            created,
            expires: if state.ttl == 0 {
                0
            } else {
                created + state.ttl
            },
            bytes: joined.into_bytes(),
            content_type: "text/plain; charset=utf-8".into(),
        };
        store(&state, item);
    }
    Redirect::to("/").into_response()
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
        .route("/api/items/{id}/share", post(create_share))
        .route("/api/drops", post(create_drop))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/app.js", get(app_js))
        .route("/style.css", get(styles))
        .route("/s/{share_id}", get(serve_share))
        .route("/d/{drop_id}", get(drop_page).post(drop_upload))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker))
        .route("/icon.svg", get(icon))
        .route("/share-target", post(share_target))
        .merge(api)
        .layer(DefaultBodyLimit::max(max_mb * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("flit server listening on {addr}");
    axum::serve(listener, app).await.unwrap();
}
