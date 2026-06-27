const listEl = document.getElementById("list");
const statusEl = document.getElementById("status");
const toastEl = document.getElementById("toast");
let lastTop = null;

function toast(m) {
  toastEl.textContent = m;
  toastEl.classList.add("show");
  setTimeout(() => toastEl.classList.remove("show"), 1200);
}

async function copy(t) {
  try {
    await navigator.clipboard.writeText(t);
    toast("복사됨");
  } catch (e) {
    toast("길게 눌러 복사");
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
  return (n / 1048576).toFixed(1) + " MB";
}

function render(items) {
  listEl.innerHTML = "";
  for (const it of items) {
    const card = document.createElement("div");
    card.className = "card";
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.innerHTML = `<span class="tag">${it.kind}</span><span>${age(it.created)} 전</span>`;
    const del = document.createElement("span");
    del.className = "del";
    del.textContent = "✕";
    del.title = "삭제";
    del.onclick = () => remove(it.id);
    meta.appendChild(del);
    card.appendChild(meta);
    if (it.kind === "file") {
      const b = document.createElement("button");
      b.className = "secondary";
      b.textContent = "⬇ " + it.name + " (" + fmtSize(it.size) + ")";
      b.onclick = () => {
        location.href = "/api/items/" + it.id + "/raw";
      };
      card.appendChild(b);
    } else {
      const pre = document.createElement("pre");
      pre.textContent = it.text || "";
      card.appendChild(pre);
      const b = document.createElement("button");
      b.className = "secondary";
      b.textContent = "복사";
      b.style.marginTop = "8px";
      b.onclick = () => copy(it.text || "");
      card.appendChild(b);
      if (it.kind === "link") {
        const o = document.createElement("a");
        o.href = it.text;
        o.target = "_blank";
        o.textContent = " 열기";
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
    document.getElementById("autocopy").checked &&
    top.kind !== "file"
  )
    copy(top.text || "");
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
    fd.append("file", f);
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
document.getElementById("file").onchange = async (e) => {
  await sendFiles(e.target.files);
  e.target.value = "";
};
document.getElementById("clear").onclick = async () => {
  if (confirm("전체 삭제할까요?")) {
    await fetch("/api/items", { method: "DELETE" });
    load();
  }
};

function connect() {
  const ev = new EventSource("/api/events");
  ev.onopen = () => {
    statusEl.textContent = "실시간 연결됨";
  };
  ev.addEventListener("item", () => refreshAndMaybeCopy());
  ev.onerror = () => {
    statusEl.textContent = "재연결 중...";
  };
}

(async () => {
  const items = await load();
  if (items && items.length) lastTop = items[0].id;
  connect();
  setInterval(load, 15000);
})();
