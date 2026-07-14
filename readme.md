**Flit Devlog #5**

Devlog #4 ended with two of the most dangerous words in software: _it's shipped._ Because "shipped" and "safe to leave running" are not the same sentence. This one is the paranoid pass — I put Flit on the open internet, immediately stopped trusting it, and spent a session fixing the things you only find _after_ strangers can reach your server: a box anyone can read, a status light that lied to my face, and a server that flat-out refused to die.

**The problem with a public box**

The demo from #4 is wide open on purpose — no login, drop something and it's there. Lovely, until you notice the flip side: _everyone_ can read _everything_. The whole identity of Flit is "no accounts," so bolting on users was off the table. The honest move was to push the secret all the way to the edge — encrypt in the browser, before a single byte leaves the device, so the server (and the nosy stranger, and me) only ever stores noise.

Web Crypto does the heavy lifting: derive a key from a passphrase with PBKDF2, then AES-256-GCM the payload.

```js
const key = await crypto.subtle.deriveKey(
  { name: "PBKDF2", salt, iterations: 150000, hash: "SHA-256" },
  base,
  { name: "AES-GCM", length: 256 },
  false,
  ["encrypt", "decrypt"],
);
```

Worked perfectly on localhost. Then I opened it on my phone over the LAN and the encrypt switch just… did nothing — no error I'd written, only a cryptic `Cannot read properties of undefined (reading 'deriveKey')`. `crypto.subtle` was straight-up _undefined_. It turns out Web Crypto only exists in a **secure context** — HTTPS or `localhost`, full stop. Plain http over a LAN IP doesn't qualify, and the browser doesn't warn you; it just quietly doesn't hand you the toy. The fix was less code than a rule written into the docs: want the lock, reach Flit over HTTPS or a VPN hostname. Localhost gets it for free.

**A status light that lied**

Small bug, deeply annoying. The inbox has a little "connecting… / connected" indicator wired to the SSE stream. On my machine it snapped to _connected_ instantly. On the deployed box it sat on _connecting…_ forever — right up until the first drop landed, at which point it finally admitted it had been connected the whole time.

I'd hung the label on `EventSource`'s `onopen`. Locally that fires the instant the socket is up. Behind Render's proxy, though, the connection is "open" but _buffered_ — nothing reaches the browser until the first real byte is flushed, and the first byte only exists once someone actually drops something. So the light was technically honest and practically useless.

The fix was to stop waiting for news and just say hello. The server now shoves a `ready` event down the pipe the moment you subscribe, before any real traffic:

```rust
let ready = tokio_stream::once(Ok(Event::default().event("ready").data("ok")));
Sse::new(ready.chain(live)).keep_alive(KeepAlive::default())
```

One guaranteed byte, flushed immediately, and the light flips the second you connect. Lesson filed: don't prove a connection by waiting for data — the connection should introduce itself.

**A server that wouldn't die**

Then the one that actually made me laugh out loud in frustration. I'd added a graceful shutdown so Ctrl+C would let in-flight uploads finish before the process exits. Sensible. Except now Ctrl+C did _nothing._ The process just sat there. `^C^C^C^C` and it kept happily listening.

The cause is almost poetic. `with_graceful_shutdown` politely waits for every open connection to close before it lets the process go. An SSE stream is a connection that, by design, _never closes_ — that is the entire point of it. So the instant a single browser tab had the inbox open, graceful shutdown had something to wait on that would never arrive. I'd built a server too polite to leave its own party.

I kept the graceful path but gave it a hard deadline — on the signal, announce it, then arm a timer that pulls the plug regardless of who's still holding a stream open:

```rust
println!("flit: shutting down");
tokio::spawn(async {
    tokio::time::sleep(Duration::from_millis(500)).await;
    std::process::exit(0);
});
```

Half a second for the well-behaved connections to wrap up, then the door shuts whether the SSE streams like it or not.

**Handing over one thing, not the keys**

The token from #4 is all-or-nothing: you either have the run of the whole inbox or you don't. But most of what I actually want is narrower — _"here's this one file,"_ or _"you, drop that screenshot to me, but you don't get to read my stuff."_ So, two more shapes of link. A **share link** wraps a single item behind its own URL (`/s/{id}`), optionally one-time or time-limited, so it evaporates after one open or once the clock runs out. A **guest drop** (`/d/{id}`) is the mirror image: a page that can only _post_, so I can hand it to someone and they can send to me without ever seeing the inbox. Same server, two new levels of trust between "stranger" and "me."

**Making it a real app**

The share-sheet trick from #4 leaned entirely on Shortcuts. This time I made Flit an actual installed thing: a PWA with a manifest and a service worker that caches the shell so the inbox opens offline, plus a **Web Share Target** so once it's installed, "Share → Flit" fires straight into the app — no Shortcut in the middle. And for the device with no keyboard, the page now paints a QR of its own address (a tiny `qrcode` render straight to SVG, with `default-features = false` so I didn't drag a whole PNG encoder in for one little code) — point a phone camera at your laptop screen and you're in.

**The small comforts**

The unglamorous polish that makes a thing feel finished: a dark mode that follows your system but takes a manual override, the whole UI in English or Korean, a rate limiter (`FLIT_RATE`) so the open box can't be machine-gunned, and an ephemeral mode (`FLIT_EPHEMERAL` + `FLIT_IDLE_SECS`) that opens your browser on launch and then quietly exits once it's sat idle long enough — a tool that eventually cleans up even its own process.

**Where Flit stands**

- **Send anything, instantly** — text, links, files over real-time SSE (#2–#3)
- **End-to-end encrypted** — scrambled in the browser; the server only holds ciphertext
- **Share links & guest drops** — hand over a single item, or a drop-only door
- **A real app** — installable PWA, offline shell, share-target, QR to join
- **Safe to leave running** — token, rate limit, auto-expiry, ephemeral idle-exit

**Next up**

Honestly? The feature list is finally where I wanted it — Flit does the whole job now, from a caveman `curl` to a scanned QR. What's left isn't more surface, it's confidence: a real test pass on the encryption round-trip and the expiry reaper, and maybe swapping the in-memory store for something that survives a restart _if_ I ever decide it should. But the pitch it started with — throw a thing here, grab it there, no accounts, no cloud — that part is done. Flit is finished, and for once I mean it.
