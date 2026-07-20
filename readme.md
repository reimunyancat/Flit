# 📡 Flit

> A frictionless cross-device drop inbox.
>
> Send text, links, or files from any device and pick them up on another — **no accounts, no cloud, no app install.** Just one small self-hosted binary on your own network.

##

---

## 🌐 Try it live

There's a public one running at <https://flit-xw2a.onrender.com> - poke at it before you bother self-hosting.

Fair warning: it's wide-open wall, so anyone can post and everyone can read it. Don't drop anything you'd mind a stranger seeing. Things auto-expire after 10 minutes, uploads cap out at 5MB, and since it's on a free box it dozes off when idle - the first hit might take a few seconds to wake it up.

## 🤔 Why

Moving a small thing between devices is weirdly annoying. A link on my phone I want on my laptop, a screenshot from my laptop I want on my home server, a snippet between two machines that don't share an OS — every option breaks my flow.

Emailing myself, DMing myself in some chat app, or fighting AirDrop (which doesn't work across Windows/Linux/iOS) gets old fast. **Flit is the boring-but-instant answer:** a shared inbox that lives on your own network.

---

## ✨ Features

- **Send anything** — plain text, URLs (auto-detected as links), or file uploads.
- **Live inbox** — a clean web page that updates in real time over SSE; new drops appear instantly on every open device.
- **Auto-copy** — opt in and the newest text/link lands straight on your clipboard the moment it arrives.
- **Self-cleaning** — items expire after a configurable TTL (default 10 minutes), so nothing piles up.
- **Optional shared token** — lock it down with a secret when you expose it beyond localhost.
- **One binary** — pure Rust (axum), no database, no runtime dependencies.
- **Send from anywhere** — a `flit` CLI for Linux/macOS, a PowerShell version for Windows, and an iOS/iPadOS Shortcut.

---

## 🚀 Quick start

```sh
cargo run --release   # listens on 0.0.0.0:7777
```

Then open <http://localhost:7777> in a browser, or throw things at the API:

```sh
curl -d "ship it" localhost:7777/api/text
curl -d "https://example.com" localhost:7777/api/text
curl -F "file=@photo.png" localhost:7777/api/file
curl -s localhost:7777/api/items | jq .
```

##

## ⚙️ Configuration

| Variable        | Default        | Meaning                                        |
| --------------- | -------------- | ---------------------------------------------- |
| `FLIT_ADDR`     | `0.0.0.0:7777` | Listen address                                 |
| `PORT`          | _(unset)_      | Overrides `FLIT_ADDR`; used by hosts like Render |
| `FLIT_TOKEN`    | _(empty)_      | Shared secret; empty = open                    |
| `FLIT_TTL_SECS` | `600`          | Item lifetime in seconds; `0` = keep forever   |
| `FLIT_MAX_MB`   | `5`            | Max upload size in MB                           |

Pass the token as a header `Authorization: Bearer <token>` or a query string `?token=<token>` (the web UI reads `?token=` from its own URL).

---

## 🔌 API

| Method | Path                  | Body      | Result                  |
| ------ | --------------------- | --------- | ----------------------- |
| `POST` | `/api/text`           | raw text  | text/link item          |
| `POST` | `/api/file`           | multipart | file item               |
| `GET`  | `/api/items`          | —         | JSON list, newest first |
| `GET`  | `/api/items/{id}/raw` | —         | original bytes          |
| `GET`  | `/api/events`         | —         | SSE stream of new items |
| `GET`  | `/`                   | —         | web inbox               |
| `GET`  | `/health`             | —         | ok                      |

---

## 💻 Clients

- **CLI (Linux/macOS):** `bin/flit` — `flit "some text"`, `flit ./file.png`, or pipe `echo hi | flit`.
- **Windows:** `bin/flit.ps1`.
- **iOS/iPadOS:** see [`shortcuts/ios-shortcut-guide.md`](shortcuts/ios-shortcut-guide.md) to add a one-tap Share Sheet action.
- **Android:** see [`shortcuts/android-guide.md`](shortcuts/android-guide.md) — same idea, via the open-source **HTTP Shortcuts** app.

  Point clients at the server with `FLIT_URL` (and `FLIT_TOKEN` if set).

---

## 📦 Deploy

Tag a release (`v*`) and GitHub Actions builds static binaries for Linux, macOS, and Windows (see [`.github/workflows/release.yml`](.github/workflows/release.yml)).

For off-network access, run it behind a mesh VPN like **NetBird** or **Tailscale** and use the overlay IP instead of exposing a port.
