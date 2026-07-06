# Flit on Android

No Shortcuts app here, but the open-source **HTTP Shortcuts** app does the same job: a share-menu action that files an HTTP request at your server.

## What you need

- **HTTP Shortcuts** (by Roland Meyer) - Play Store or F-Droid, open source.
- Your Flit server URL, no trailing slash.
- The token, only if your server sets `FLIT_TOKEN`.

## Shortcut 1 - text & links

1. New shortcut, name it _Flit text_.
2. **Method:** POST - **URL:** `https://<your-server>/api/text`
3. **Body:** plain text, content set to the app's shared-text variable.
4. **Header** (only if a token is set): `Authorization` = `Bearer <token>`
5. Enable the option that puts the shortcut in Android's share menu.

## Shortcut 2 - files & photos

Same as above, except:

- **URL:** `https://<your-server>/api/file`
- **Body:** form-data -> one parameter of type **file**, named exactly `file`, value = the shared file.

## Use it

Share anything -> **HTTP Shortcuts** -> _Flit file_. It lands in the inbox on every open device.

## If it doesn't work

- **401 unauthorized** -> token missing or wrong.
- **Can't reach the server off your home network** -> mesh VPN (NetBird / Tailscale), use the overlay IP as your server URL.
- **"no file field"** -> the file parameter must be named exactly `file`.
