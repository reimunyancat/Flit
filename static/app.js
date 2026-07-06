const LANG = (localStorage.getItem("flit_lang") || navigator.language || "en")
  .toLowerCase()
  .startsWith("ko")
  ? "ko"
  : "en";
const STR = {
  ko: {
    autocopyLabel: "자동 복사",
    clear: "전체 지우기",
    send: "보내기",
    file: "파일",
    placeholder: "텍스트나 링크를 붙여넣고 Enter...",
    connecting: "연결 중...",
    connected: "실시간 연결됨",
    reconnecting: "재연결 중...",
    copied: "복사됨",
    autoCopied: "자동 복사됨",
    imgCopied: "이미지 복사됨",
    imgAutoCopied: "이미지 자동 복사됨",
    longPress: "길게 눌러 복사",
    imgUnsupported: "이미지 복사 불가",
    imgSent: "이미지 보냄",
    fileSent: "파일 보냄",
    dropHere: "여기에 놓으세요",
    ago: "전",
    del: "삭제",
    copy: "복사",
    copyImg: "이미지 복사",
    open: "열기",
    confirmClear: "전체 삭제할까요?",
  },
  en: {
    autocopyLabel: "Auto-copy",
    clear: "Clear all",
    send: "Send",
    file: "File",
    placeholder: "Paste text or a link, then Enter...",
    connecting: "Connecting...",
    connected: "Live",
    reconnecting: "Reconnecting...",
    copied: "Copied",
    autoCopied: "Auto-copied",
    imgCopied: "Image copied",
    imgAutoCopied: "Image auto-copied",
    longPress: "Long-press to copy",
    imgUnsupported: "Can't copy image",
    imgSent: "Image sent",
    fileSent: "File sent",
    dropHere: "Drop to send",
    ago: "ago",
    del: "Delete",
    copy: "Copy",
    copyImg: "Copy image",
    open: "Open",
    confirmClear: "Clear everything?",
  },
};
function t(k) {
  return (STR[LANG] && STR[LANG][k]) || STR.en[k] || k;
}
function applyI18n() {
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.getAttribute("data-i18n"));
  });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => {
    el.placeholder = t(el.getAttribute("data-i18n-ph"));
  });
  const lb = document.getElementById("lang");
  if (lb) lb.textContent = LANG === "ko" ? "EN" : "한";
}
function setLang(l) {
  localStorage.setItem("flit_lang", l);
  location.reload();
}

const listEl = document.getElementById("list");
const statusEl = document.getElementById("status");
const toastEl = document.getElementById("toast");
let lastTop = null;
let pendingCopy = null;

function toast(m) {
  toastEl.textContent = m;
  toastEl.classList.add("show");
  setTimeout(() => toastEl.classList.remove("show"), 1200);
}

async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast(t("copied"));
  } catch (e) {
    toast(t("longPress"));
  }
}

function isImage(name) {
  return /\.(png|jpe?g|gif|webp|bmp|svg|avif)$/i.test(name || "");
}

function isPdf(name) {
  return /\.pdf$/i.test(name || "");
}

async function copyImage(id) {
  try {
    const blob = await fetch("/api/items/" + id + "/raw").then((r) => r.blob());
    await navigator.clipboard.write([new ClipboardItem({ [blob.type]: blob })]);
    toast(t("imgCopied"));
  } catch (e) {
    toast(t("imgUnsupported"));
  }
}

async function doCopy(item) {
  try {
    if (item.kind === "file") {
      if (!isImage(item.name)) return false;
      const blob = await fetch("/api/items/" + item.id + "/raw").then((r) =>
        r.blob(),
      );
      await navigator.clipboard.write([
        new ClipboardItem({ [blob.type]: blob }),
      ]);
      toast(t("imgAutoCopied"));
    } else {
      await navigator.clipboard.writeText(item.text || "");
      toast(t("autoCopied"));
    }
    return true;
  } catch (e) {
    return false;
  }
}

async function autocopy(item) {
  if (item.kind === "file" && !isImage(item.name)) return;
  if (document.hasFocus() && (await doCopy(item))) return;
  pendingCopy = item;
}

async function flushPending() {
  if (!pendingCopy) return;
  const it = pendingCopy;
  if (await doCopy(it)) pendingCopy = null;
}

window.addEventListener("focus", flushPending);
document.addEventListener("pointerdown", flushPending);
document.addEventListener("keydown", flushPending);

function age(s) {
  const d = Math.max(0, Math.floor(Date.now() / 1000) - s);
  if (d < 60) return d + "s";
  if (d < 3600) return Math.floor(d / 60) + "m";
  return Math.floor(d / 3600) + "h";
}

function fmtSize(n) {
  if (n < 1024) return n + " B";
  if (n < 1048576) return (n / 1024).toFixed(1) + " KB";
  return (n / 1048576).toFixed(1) + " MB";
}

function render(items) {
  listEl.innerHTML = "";
  for (const it of items) {
    const card = document.createElement("div");
    card.className = "card";
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.innerHTML = `<span class="tag">${it.kind}</span><span>${age(it.created)} ${t("ago")}</span>`;
    const del = document.createElement("span");
    del.className = "del";
    del.textContent = "✕";
    del.title = t("del");
    del.onclick = () => remove(it.id);
    meta.appendChild(del);
    card.appendChild(meta);
    if (it.kind === "file") {
      const raw = "/api/items/" + it.id + "/raw";
      if (isImage(it.name)) {
        const img = document.createElement("img");
        img.src = raw;
        img.alt = it.name;
        img.style.cssText =
          "display:block;max-width:100%;max-height:320px;border-radius:8px;margin-bottom:8px;cursor:pointer";
        img.onclick = () => window.open(raw, "_blank");
        card.appendChild(img);
      } else if (isPdf(it.name)) {
        const pv = document.createElement("embed");
        pv.src = raw;
        pv.type = "application/pdf";
        pv.style.cssText =
          "display:block;width:100%;height:360px;border:1px solid #2a2a2a;border-radius:8px;margin-bottom:8px";
        card.appendChild(pv);
      }
      const b = document.createElement("button");
      b.className = "secondary";
      b.textContent = "⬇ " + it.name + " (" + fmtSize(it.size) + ")";
      b.onclick = () => {
        location.href = "/api/items/" + it.id + "/raw";
      };
      card.appendChild(b);
      if (isImage(it.name)) {
        const c = document.createElement("button");
        c.className = "secondary";
        c.textContent = t("copyImg");
        c.style.marginLeft = "8px";
        c.onclick = () => copyImage(it.id);
        card.appendChild(c);
      }
    } else {
      const pre = document.createElement("pre");
      pre.textContent = it.text || "";
      card.appendChild(pre);
      const b = document.createElement("button");
      b.className = "secondary";
      b.textContent = t("copy");
      b.style.marginTop = "8px";
      b.onclick = () => copy(it.text || "");
      card.appendChild(b);
      if (it.kind === "link") {
        const o = document.createElement("a");
        o.href = it.text;
        o.target = "_blank";
        o.textContent = " " + t("open");
        o.style.marginLeft = "8px";
        card.appendChild(o);
      }
    }
    listEl.appendChild(card);
  }
}

async function load() {
  try {
    const r = await fetch("/api/items");
    const items = await r.json();
    render(items);
    return items;
  } catch (e) {
    return null;
  }
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
    document.getElementById("autocopy").checked
  )
    autocopy(top);
  lastTop = top.id;
}

async function remove(id) {
  await fetch("/api/items/" + id, { method: "DELETE" });
  load();
}

async function send() {
  const ta = document.getElementById("text");
  const v = ta.value;
  if (!v.trim()) return;
  await fetch("/api/text", {
    method: "POST",
    headers: { "Content-Type": "text/plain" },
    body: v,
  });
  ta.value = "";
}

async function sendFiles(files) {
  for (const f of files) {
    const fd = new FormData();
    const name =
      f.name && f.name !== "blob"
        ? f.name
        : "paste-" + Date.now() + "." + (f.type.split("/")[1] || "bin");
    fd.append("file", f, name);
    await fetch("/api/file", { method: "POST", body: fd });
  }
}

document.getElementById("send").onclick = send;
document.getElementById("text").addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    send();
  }
});
document.addEventListener("paste", (e) => {
  const items = (e.clipboardData && e.clipboardData.items) || [];
  const files = [];
  for (const it of items) {
    if (it.kind === "file") {
      const f = it.getAsFile();
      if (f) files.push(f);
    }
  }
  if (files.length) {
    e.preventDefault();
    sendFiles(files);
    toast(t("fileSent"));
  }
});
document.getElementById("file").onchange = async (e) => {
  await sendFiles(e.target.files);
  e.target.value = "";
};
document.getElementById("clear").onclick = async () => {
  if (confirm(t("confirmClear"))) {
    await fetch("/api/items", { method: "DELETE" });
    load();
  }
};
document.getElementById("lang").onclick = () =>
  setLang(LANG === "ko" ? "en" : "ko");

const autocopyEl = document.getElementById("autocopy");
autocopyEl.checked = localStorage.getItem("flit_autocopy") !== "0";
autocopyEl.addEventListener("change", () => {
  localStorage.setItem("flit_autocopy", autocopyEl.checked ? "1" : "0");
});

function connect() {
  const ev = new EventSource("/api/events");
  ev.onopen = () => {
    statusEl.textContent = t("connected");
  };
  ev.addEventListener("item", () => refreshAndMaybeCopy());
  ev.onerror = () => {
    statusEl.textContent = t("reconnecting");
  };
}

const dropzone = document.getElementById("dropzone");
let dragDepth = 0;
window.addEventListener("dragenter", (e) => {
  e.preventDefault();
  dragDepth++;
  dropzone.classList.add("show");
});
window.addEventListener("dragover", (e) => {
  e.preventDefault();
});
window.addEventListener("dragleave", (e) => {
  e.preventDefault();
  dragDepth = Math.max(0, dragDepth - 1);
  if (dragDepth === 0) dropzone.classList.remove("show");
});
window.addEventListener("drop", (e) => {
  e.preventDefault();
  dragDepth = 0;
  dropzone.classList.remove("show");
  const files = e.dataTransfer && e.dataTransfer.files;
  if (files && files.length) {
    sendFiles(files);
    toast(t("fileSent"));
  }
});
(async () => {
  applyI18n();
  const items = await load();
  if (items && items.length) lastTop = items[0].id;
  connect();
  setInterval(load, 15000);
})();
