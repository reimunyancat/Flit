# Flit on iOS & iPadOS

Flit doesn't ship an app. On iPhone and iPad the client is a **Shortcut** - an action you add to the share sheet so "Share -> Flit" throws whatever you're looking at into your inbox.

## What you need

- The **Shortcut** app (built into iOS/iPadOS).
- Your Flit server URL, no trailing slash - e.g. `https://flit-xw2a.onrender.com`, or a LAN/VPN address.
- The token, only if your server sets `FLIT_TOKEN`.

## Shortcut 1 - text & links

1. Shortcut -> **+** -> rename it _Flit text_
2. Add the action **Get Contents of URL**: -**URL:** `https://<your-server>/api/text` -**Method:** POST -**Request Body:** Text, set to the **Shortcut Input** (whatever gets shared) -**Header** (only if a token is set): `Authorization` = `Bearer <token>`
3. In the shortcut's settings, turn on **Show in Share Sheet** and accept Text + URLs.

## Shortcut 2 - files & photos

Same as above, except:

- **URL:** `https://<your-server>/api/file`
- **Request Body:** Form -> add one field of type **File**,named exactly `file`, value = **Shortcut Input**
- Accept Images / Files in the share sheet.

## Use it

Anywhere - Photos, Safari, Files - tap **Share**, pick _Flit text_ or _Flit file_. It lands in the inbox on every open device.

## If it doesn't work

- **401 unauthorized** -> token missing or wrong.
- **Can't reach the server off your home network** -> put both devices on the mesh VPN (NetBird / Tailscale) and use the overlay IP as your server URL.
- **"no file field"** -> the file field must be named exactly `file`.
