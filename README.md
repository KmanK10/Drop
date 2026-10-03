# Drop

A private clipboard for a few people on your own server. Paste text or drop a file in a signed-in browser and it shows up on that person's other devices. Accounts are separate. The server stores ciphertext, not the items.

Licensed under the MIT license.

## Run

One container. The setup secret is not baked into the image. Until the first admin exists, `SETUP_SECRET` must be at least 16 characters. After that it is ignored.

```bash
export SETUP_SECRET="$(openssl rand -base64 32)"
docker compose up -d --build
```

Point a reverse proxy at the published port with HTTPS and WebSockets enabled. One example of `PUBLIC_URL` is [https://drop.kiefermenard.com](https://drop.kiefermenard.com). The proxy should send `X-Forwarded-Proto`, `X-Forwarded-Host`, and `X-Real-IP`. Devices stay in sync on the same origin at `/api/ws`.

Open `/setup` once, enter the setup secret, and choose the admin username and password. The password is not the setup secret.

To try it on the machine itself, without TLS:

```bash
docker build -t drop .
docker run --rm -p 8080:8080 \
  -e SETUP_SECRET \
  -e PUBLIC_URL=http://127.0.0.1:8080 \
  -e COOKIE_SECURE=false \
  -e TRUST_PROXY=false \
  -v drop-data:/data \
  drop
```

### Environment

| Variable | Purpose |
| --- | --- |
| `SETUP_SECRET` | Required until the first admin exists. Not a password, and not stored. |
| `PUBLIC_URL` | Origin used in invite links. The image default is `https://drop.kiefermenard.com`. |
| `COOKIE_SECURE` | `true` when the browser uses HTTPS, including through a proxy. |
| `TRUST_PROXY` | `true` behind a reverse proxy so the forwarded host and client IP are used. |
| `DATA_DIR` | SQLite directory. `/data` in the container. |
| `PORT` / `HOST` | Listen address. Defaults `8080` and `0.0.0.0`. |

There is no public signup. An admin creates a username under People and gets a one-time link, valid for 48 hours. The raw link is not stored. Send it to the person. They choose their own password. Choose the Admin role only for someone who should also be able to invite. If the link is lost, revoke the invite and create another. A proxy that logs URLs can see a link until it is used.

A device stays signed in for 30 days of use, up to 180 days. Decrypting still asks for the password on that device: the content key is held in memory for the tab, not on the server. A new device, or a reloaded tab, asks again.

Each account has a storage quota. New accounts get 5 GB. An admin can set any account, including their own, from 1 KB to 32 GB. The server rejects a larger quota, and it rejects an upload whose ciphertext would push the account over its quota. There is no separate item count or per-file cap. Names and file bytes are inside the ciphertext, so the quota is the sum of those blobs.

Items are deleted 30 days after they are saved. That is a hard delete: the ciphertext is removed, same as deleting by hand. There is no trash. The invite page says so before a new person chooses a password, and the clipboard shows the quota and the 30 days beside it.

An admin can change someone's role later, and can delete an account. Deleting an account deletes that person's items. The last admin cannot be demoted or deleted. If another admin exists, an admin can delete their own account.

Anyone can change their own password from Settings. The browser re-encrypts existing items under the new key, then the server stores the new verifier and drops every other session. The device that changed the password stays signed in. The server still never sees the password or either content key.

Logs record the method, route, and status, not bodies, passwords, or invite tokens.

Backups of `/data` are backups of ciphertext. Stop the container and copy the volume, or copy `drop.sqlite` together with its `-wal` and `-shm` files. Old backups still contain old ciphertext, which remains unreadable without the password.

## Development

```bash
export SETUP_SECRET="$(openssl rand -base64 32)"
npm install
npm test
npm run dev
```

The dev server is [http://127.0.0.1:43123](http://127.0.0.1:43123). Set `SETUP_SECRET` in the environment before the first admin exists. It is not written into the image. `npm test` checks the Argon2id split, encryption, and the auth boundaries: setup happens once, invites are single-use, users cannot read each other's items, a quota is enforced, expired items are removed, the last admin cannot be removed, a password change still decrypts old items, and a second session hears about changes over the WebSocket.

## How the key is split

Argon2id (19 MiB, 2 iterations) runs in the browser and produces 64 bytes. The first 32 are hashed to form the auth verifier sent at signup and login. The server stores only a hash of that verifier, plus the salt and KDF parameters. The last 32 bytes are the AES-256-GCM content key. They are used to encrypt the item and a small key-check, then discarded from the signup request. Unlocking checks the key-check locally and does not send the password.
