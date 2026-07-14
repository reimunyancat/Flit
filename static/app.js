function getCookie(n) {
  const m = document.cookie.match("(?:^|; )" + n + "=([^;]*)");
  return m ? decodeURIComponent(m[1]) : "";
}

const urlToken = new URLSearchParams(location.search).get("token") || "";
if (urlToken) {
  document.cookie =
    "flit_token=" +
    encodeURIComponent(urlToken) +
    ";path=/;max-age=31536000;samesite=lax";
}
const token = urlToken || getCookie("flit_token");
const auth = token ? { Authorization: "Bearer " + token } : {};
const qs = token ? "?token=" + encodeURIComponent(token) : "";
const listEl = document.getElementById("list"),
  statusEl = document.getElementById("status"),
  toastEl = document.getElementById("toast"),
  passEl = document.getElementById("pass");
const ENC = "FLITENC1:";
let lastTop = null;
const I18N = {
  en: {
    autocopy: "Auto-copy",
    pass_ph: "Passphrase (E2E)",
    pair: "QR",
    drop: "Drop link",
    clear: "Clear all",
    connecting: "Connecting...",
    connected: "Live",
    reconnecting: "Reconnecting...",
    compose_ph: "Paste text or a link, hit Enter...",
    send: "Send",
    file: "File",
    scan: "Scan from another device",
    close: "Close",
    copied: "Copied",
    copy_fail: "Long-press to copy",
    copy: "Copy",
    open: "Open",
    ago: "ago",
    encrypted: "Encrypted",
    enc_locked: "Enter the passphrase to unlock",
    enc_need_pass: "Enter the passphrase",
    enc_mismatch: "Wrong passphrase",
    auth_needed: "Auth required (?token=)",
    upload_fail: "Upload failed: ",
    share: "Share",
    del: "Delete",
    share_fail: "Share failed",
    share_copied: "Share link copied",
    drop_fail: "Couldn't create drop link",
    drop_copied: "Drop link copied - send it to them",
    confirm_clear: "Delete everything?",
    pasted: "{n} pasted",
    theme_dark: "Dark",
    theme_light: "Light",
  },
  ko: {
    autocopy: "자동복사",
    pass_ph: "암호(E2E)",
    pair: "QR",
    drop: "받기 링크",
    clear: "전체 비우기",
    connecting: "연결 중...",
    connected: "실시간 연결됨",
    reconnecting: "재연결 중...",
    compose_ph: "텍스트나 링크를 붙여넣고 Enter...",
    send: "보내기",
    file: "파일",
    scan: "다른 기기에서 스캔",
    close: "닫기",
    copied: "복사됨",
    copy_fail: "길게 눌러 복사",
    copy: "복사",
    open: "열기",
    ago: "전",
    encrypted: "암호화됨",
    enc_locked: "암호를 입력하면 풀립니다",
    enc_need_pass: "암호를 입력하세요",
    enc_mismatch: "암호 불일치",
    auth_needed: "인증 필요 (?token=)",
    upload_fail: "업로드 실패: ",
    share: "공유",
    del: "삭제",
    share_fail: "공유 실패",
    share_copied: "공유 링크 복사됨",
    drop_fail: "받기 링크 생성 실패",
    drop_copied: "받기 링크 복사됨 - 상대에게 보내세요",
    confirm_clear: "전체 삭제?",
    pasted: "{n}개 붙여넣기 전송",
    theme_dark: "다크",
    theme_light: "라이트",
  },
};
let LANG =
  localStorage.getItem("flit_lang") ||
  ((navigator.language || "en").toLowerCase().startsWith("ko") ? "ko" : "en");
let THEME =
  localStorage.getItem("flit_theme") ||
  (window.matchMedia &&
  window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light");
let connState = "connecting";
const tr = (k) => (I18N[LANG] && I18N[LANG][k]) || I18N.en[k] || k;
function renderStatus() {
  if (statusEl) statusEl.textContent = tr(connState);
}
function applyTheme() {
  document.documentElement.setAttribute("data-theme", THEME);
  const tb = document.getElementById("theme");
  if (tb)
    tb.textContent = THEME === "dark" ? tr("theme_light") : tr("theme_dark");
}
function applyI18n() {
  document.documentElement.lang = LANG;
  const lb = document.getElementById("lang");
  if (lb) lb.textContent = LANG === "ko" ? "English" : "한국어";
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = tr(el.getAttribute("data-i18n"));
  });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => {
    el.placeholder = tr(el.getAttribute("data-i18n-ph"));
  });
  applyTheme();
  renderStatus();
}
applyI18n();

function toast(m) {
  toastEl.textContent = m;
  toastEl.classList.add("show");
  setTimeout(() => toastEl.classList.remove("show"), 1200);
}

async function copy(t) {
  try {
    await navigator.clipboard.writeText(t);
    toast(tr("copied"));
  } catch (e) {
    toast(tr("copy_fail"));
  }
}

function age(s) {
  const d = Math.max(0, Math.floor(Date.now() / 1000) - s);
  if (d < 60) return d + "s";
  if (d < 3600) return Math.floor(d / 60) + "m";
  return Math.floor(d / 3600) + "h";
}

function fmtSize(n) {
  if (n < 1024) return n + " B";
  if (n < 1048576) return (n / 1024).toFixed(1) + " KB";
  if (n < 1073741824) return (n / 1048576).toFixed(1) + " MB";
  return (n < 1073741824).toFixed(2) + " GB";
}

function isImg(name) {
  return (
    /\.(png|jpe?g|gif|webp|bmp|svg|avif)$/i.test(name) &&
    !name.endsWith(".flitenc")
  );
}

function isPdf(name) {
  return /\.pdf$/i.test(name) && !name.endsWith(".flitenc");
}

function b64(b) {
  let s = "";
  for (const x of b) s += String.fromCharCode(x);
  return btoa(s);
}

function unb64(t) {
  const s = atob(t);
  const a = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) a[i] = s.charCodeAt(i);
  return a;
}

async function deriveKey(pass, salt) {
  const base = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(pass),
    "PBKDF2",
    false,
    ["deriveKey"],
  );
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations: 150000, hash: "SHA-256" },
    base,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

async function encBytes(pass, bytes) {
  const salt = crypto.getRandomValues(new Uint8Array(16)),
    iv = crypto.getRandomValues(new Uint8Array(12));
  const key = await deriveKey(pass, salt);
  const ct = new Uint8Array(
    await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, bytes),
  );
  const out = new Uint8Array(28 + ct.length);
  out.set(salt, 0);
  out.set(iv, 16);
  out.set(ct, 28);
  return out;
}

async function decBytes(pass, buf) {
  const salt = buf.slice(0, 16),
    iv = buf.slice(16, 28),
    ct = buf.slice(28);
  const key = await deriveKey(pass, salt);
  return new Uint8Array(
    await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, ct),
  );
}

async function decText(it) {
  if (!(it.text && it.text.startsWith(ENC))) return it.text || "";
  const pass = passEl.value;
  if (!pass) return null;
  try {
    return new TextDecoder().decode(
      await decBytes(pass, unb64(it.text.slice(ENC.length))),
    );
  } catch (e) {
    return null;
  }
}

async function render(items) {
  listEl.innerHTML = "";
  for (const it of items) {
    const card = document.createElement("div");
    card.className = "card";
    const meta = document.createElement("div");
    meta.className = "meta";
    const enc =
      (it.kind !== "file" && it.text && it.text.startsWith(ENC)) ||
      (it.kind === "file" && it.name.endsWith(".flitenc"));
    meta.innerHTML =
      `<span class="tag">${it.kind}</span><span>${age(it.created)} ${tr("ago")}</span>` +
      (enc ? `<span title="${tr("encrypted")}">🔒</span>` : "");
    const del = document.createElement("span");
    del.className = "del";
    del.textContent = "X";
    del.title = tr("del");
    del.onclick = () => remove(it.id);
    meta.appendChild(del);
    card.appendChild(meta);
    if (it.kind === "file") {
      const name = it.name.endsWith(".flitenc")
        ? it.name.slice(0, -8)
        : it.name;
      if (isImg(it.name)) {
        const im = document.createElement("img");
        im.src = "/api/items/" + it.id + "/raw" + qs;
        im.style.maxWidth = "220px";
        im.style.maxHeight = "220px";
        im.style.borderRadius = "10px";
        im.style.display = "block";
        im.style.margin = "4px 0 8px";
        im.loading = "lazy";
        card.appendChild(im);
      } else if (isPdf(it.name)) {
        const fr = document.createElement("iframe");
        fr.src = "/api/items/" + it.id + "/raw" + qs;
        fr.style.width = "100%";
        fr.style.height = "320px";
        fr.style.border = "0";
        fr.style.borderRadius = "10px";
        fr.style.margin = "4px 0 8px";
        card.appendChild(fr);
      }
      const b = document.createElement("button");
      b.className = "secondary";
      b.textContent = "⬇ " + name + " (" + fmtSize(it.size) + ")";
      b.onclick = () => downloadItem(it);
      card.appendChild(b);
    } else {
      const pre = document.createElement("pre");
      card.appendChild(pre);
      const txt = await decText(it);
      if (txt === null) pre.textContent = tr("enc_locked");
      else {
        pre.textContent = txt;
        const b = document.createElement("button");
        b.className = "secondary";
        b.textContent = tr("copy");
        b.style.marginTop = "8px";
        b.onclick = () => copy(txt);
        card.appendChild(b);
        if (it.kind === "link") {
          const o = document.createElement("a");
          o.href = txt;
          o.target = "_blank";
          o.textContent = " " + tr("open");
          o.style.marginLeft = "8px";
          card.appendChild(o);
        }
      }
    }
    const sh = document.createElement("button");
    sh.className = "secondary";
    sh.textContent = tr("share");
    sh.style.marginTop = "8px";
    sh.style.marginLeft = "8px";
    sh.onclick = () => shareItem(it.id);
    card.appendChild(sh);
    listEl.appendChild(card);
  }
}

async function load() {
  const r = await fetch("/api/items", { headers: auth });
  if (!r.ok) {
    statusEl.textContent = tr("auth_needed");
    return null;
  }
  const items = await r.json();
  await render(items);
  return items;
}

async function refreshAndMaybeCopy() {
  const items = await load();
  if (!items || !items.length) {
    lastTop = null;
    return;
  }
  const top = items[0];
  if (
    lastTop &&
    top.id !== lastTop &&
    document.getElementById("autocopy").checked &&
    top.kind !== "file"
  ) {
    const txt = await decText(top);
    if (txt) copy(txt);
  }
  lastTop = top.id;
}

async function send() {
  const ta = document.getElementById("text");
  const v = ta.value;
  if (!v.trim()) return;
  const pass = passEl.value;
  let body = v;
  if (pass) body = ENC + b64(await encBytes(pass, new TextEncoder().encode(v)));
  await fetch("/api/text", {
    method: "POST",
    headers: { ...auth, "Content-Type": "text/plain" },
    body,
  });
  ta.value = "";
}

function uploadXHR(fd, onprog) {
  return new Promise((res, rej) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", "/api/file" + qs);
    if (token) xhr.setRequestHeader("Authorization", "Bearer " + token);
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable) onprog(e.loaded / e.total);
    };
    xhr.onload = () => {
      xhr.status >= 200 && xhr.status < 300
        ? res()
        : rej(new Error("HTTP " + xhr.status));
    };
    xhr.onerror = () => rej(new Error("network"));
    xhr.send(fd);
  });
}

async function sendFiles(files) {
  const pass = passEl.value;
  for (const f of files) {
    let blob = f,
      name = f.name;
    if (pass) {
      const buf = new Uint8Array(await f.arrayBuffer());
      blob = new Blob([await encBytes(pass, buf)]);
      name = f.name + ".flitenc";
    }
    const fd = new FormData();
    fd.append("file", blob, name);
    try {
      await uploadXHR(fd, (p) => {
        statusEl.textContent = "⬆ " + name + " " + Math.round(p * 100) + "%";
      });
      statusEl.textContent = tr("connected");
    } catch (err) {
      toast(tr("upload_fail") + name);
    }
  }
}

async function downloadItem(it) {
  const r = await fetch("/api/items/" + it.id + "/raw" + qs, { headers: auth });
  let bytes = new Uint8Array(await r.arrayBuffer()),
    name = it.name;
  if (it.name.endsWith(".flitenc")) {
    const pass = passEl.value;
    if (!pass) {
      toast(tr("enc_need_pass"));
      return;
    }
    try {
      bytes = await decBytes(pass, bytes);
      name = it.name.slice(0, -8);
    } catch (e) {
      toast(tr("enc_mismatch"));
      return;
    }
  }
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([bytes]));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

async function remove(id) {
  await fetch("/api/items/" + id, { method: "DELETE", headers: auth });
  load();
}

async function shareItem(id) {
  const r = await fetch("/api/items/" + id + "/share", {
    method: "POST",
    headers: { ...auth, "Content-Type": "application/json" },
    body: "{}",
  });
  if (!r.ok) {
    toast(tr("share_fail"));
    return;
  }
  const j = await r.json();
  await copy(j.url);
  toast(tr("drop_copied"));
}

async function makeDrop() {
  const r = await fetch("/api/drops", {
    method: "POST",
    headers: { ...auth, "Content-Type": "application/json" },
    body: JSON.stringify({ label: "guest" }),
  });
  if (!r.ok) {
    toast(tr("drop_fail"));
    return;
  }
  const j = await r.json;
  await copy(j.url);
  toast(tr("drop_copied"));
}

document.getElementById("send").onclick = send;
document.getElementById("text").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    send();
  }
});
document.getElementById("file").onchange = async (e) => {
  await sendFiles(e.target.files);
  e.target.value = "";
};
let dragDepth = 0;
window.addEventListener("dragover", (e) => {
  e.preventDefault();
});
window.addEventListener("dragenter", (e) => {
  e.preventDefault();
  dragDepth++;
  document.body.style.outline = "3px dashed #2dd672";
  document.body.style.outlineOffset = "-10px";
});
window.addEventListener("dragleave", (e) => {
  e.preventDefault();
  if (--dragDepth <= 0) {
    dragDepth = 0;
    document.body.style.outline = "";
  }
});
window.addEventListener("drop", async (e) => {
  e.preventDefault();
  dragDepth = 0;
  document.body.style.outline = "";
  if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length)
    await sendFiles(e.dataTransfer.files);
});
window.addEventListener("paste", async (e) => {
  const items = (e.clipboardData || {}).items || [];
  const files = [];
  for (const it of items) {
    if (it.kind === "file") {
      const f = it.getAsFile();
      if (f) files.push(f);
    }
  }
  if (files.length) {
    e.preventDefault();
    await sendFiles(files);
    toast(tr("pasted").replace("{n}", files.length));
  }
});
document.getElementById("clear").onclick = async () => {
  if (confirm(tr("confirm_clear"))) {
    await fetch("/api/items", { method: "DELETE", headers: auth });
    load();
  }
};
load();
document.getElementById("drop").onclick = makeDrop;
document.getElementById("lang").onclick = () => {
  LANG = LANG === "ko" ? "en" : "ko";
  localStorage.setItem("flit_lang", LANG);
  applyI18n();
  load();
};
document.getElementById("theme").onclick = () => {
  THEME = THEME === "dark" ? "light" : "dark";
  localStorage.setItem("flit_theme", THEME);
  applyTheme();
};
document.getElementById("pair").onclick = async () => {
  let base = location.origin;
  try {
    const r = await fetch("/api/info" + qs, { headers: auth });
    if (r.ok) {
      const j = await r.json();
      if (j.url) base = j.url;
    }
  } catch (e) {}
  document.getElementById("qrimg").src = "/qr" + qs;
  document.getElementById("pairurl").textContent = base + "/" + qs;
  document.getElementById("modal").classList.add("show");
};
passEl.addEventListener("change", load);
function connect() {
  const ev = new EventSource("/api/events" + qs);
  ev.onopen = () => {
    connState = "connected";
    renderStatus();
  };
  ev.addEventListener("item", () => refreshAndMaybeCopy());
  ev.onerror = () => {
    connState = "reconnecting";
    renderStatus();
  };
}
(async () => {
  const items = await load();
  if (items && items.length) lastTop = items[0].id;
  connect();
  setInterval(load, 15000);
})();
if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}
