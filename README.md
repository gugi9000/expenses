# Udgifter

Mobile-first web app for tracking expenses. Users upload vouchers (*bilag*: receipts, bills, invoices), fill in the details, and collect them into expense sheets. The UI is in Danish.

- Rust: [Leptos](https://leptos.dev) 0.8 (server-side rendering + WASM hydration) on Axum
- SQLite, with uploaded files stored on local disk
- Login with Microsoft Entra ID (any user in the tenant) or local users created with `create-user`
- Foreign currencies are converted to DKK using ECB reference rates

## Development

Prerequisites: Rust (stable), the WASM target and [cargo-leptos](https://github.com/leptos-rs/cargo-leptos).

```sh
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --locked   # see note below for Windows ARM64
cp .env.example .env                  # then edit
cargo run --features ssr --bin create-user -- --username admin --role admin
cargo leptos watch                    # http://localhost:3000
```

Tests: `cargo test --features ssr`

> **Windows ARM64:** `cargo install cargo-leptos` fails because vendored OpenSSL needs Perl. Install Strawberry Perl, or download the prebuilt `cargo-leptos-x86_64-pc-windows-msvc` binary from the [releases page](https://github.com/leptos-rs/cargo-leptos/releases) and place it in `~/.cargo/bin` (it runs under x64 emulation).

## Deployment

The app is a single server binary plus a `site/` folder of static assets (WASM, JS, CSS). It listens on plain HTTP on localhost. A reverse proxy (Caddy in the examples) provides HTTPS. HTTPS is required: the phone camera only works on secure pages, and session cookies are marked `Secure`.

```
Internet ──HTTPS──> Caddy :443 ──HTTP──> expenses 127.0.0.1:3000 ──> DATA_DIR/{expenses.db, files/}
```

The server must be able to reach `login.microsoftonline.com` (sign-in) and `www.ecb.europa.eu` (exchange rates) over outbound HTTPS.

### 1. Build

Build on the server, or on another machine with the same OS and CPU architecture:

```sh
cargo leptos build --release
cargo build --release --features ssr --bin create-user
```

The deployable files are:

| From | To (Linux / Windows) |
|---|---|
| `target/release/expenses[.exe]` | `/opt/udgifter/` / `C:\Udgifter\` |
| `target/release/create-user[.exe]` | `/opt/udgifter/` / `C:\Udgifter\` |
| `target/site/` (whole folder) | `/opt/udgifter/site/` / `C:\Udgifter\site\` |

Nothing else from the repository is needed at runtime, because database migrations are compiled into the binary.

### 2. Configure

All settings come from environment variables. At startup the server also reads a `.env` file from its working directory (it does not override variables that are already set). Production example (on Windows use `DATA_DIR=C:\Udgifter\data`):

```ini
BASE_URL=https://udgifter.example.dk
DATA_DIR=/var/lib/udgifter
TRUST_PROXY=true
SESSION_HOURS=12
RUST_LOG=info,sqlx=warn

ENTRA_TENANT_ID=<directory (tenant) GUID>
ENTRA_CLIENT_ID=<application (client) ID>
ENTRA_CLIENT_SECRET=<client secret value>
ENTRA_ADMIN_ROLE=Expenses.Admin

LEPTOS_OUTPUT_NAME=expenses
LEPTOS_SITE_ROOT=site
LEPTOS_SITE_PKG_DIR=pkg
LEPTOS_SITE_ADDR=127.0.0.1:3000
LEPTOS_ENV=PROD
```

- `BASE_URL` must be exactly the origin users see in the browser (scheme + host, no trailing slash). Requests that change data are rejected when their `Origin` header doesn't match it.
- Only set `TRUST_PROXY=true` behind a reverse proxy. The audit log then records the client IP from `X-Forwarded-For` instead of the proxy's address.
- `LEPTOS_SITE_ROOT` is resolved from the working directory, so run the service from the install folder.
- Don't put comments on the same line as a value: systemd's `EnvironmentFile` would treat them as part of the value.
- The file contains the client secret: make it readable only by the service account.

### 3. Microsoft Entra ID

In the [Entra admin center](https://entra.microsoft.com) → **App registrations** → your app:

1. **Authentication** → add platform **Web** with the redirect URI `https://udgifter.example.dk/auth/entra/callback`.
2. **Certificates & secrets** → create a client secret and put its **Value** (not the Secret ID) in `ENTRA_CLIENT_SECRET`. Note its expiry date, because sign-in stops working when the secret expires.
3. **App roles** → create a role with display name *Expenses Admin*, value `Expenses.Admin`, and allowed member types *Users/Groups*.
4. **Enterprise applications** → the same app → **Users and groups** → assign the role to a security group (e.g. "Udgifter – administratorer").
5. Optional: under **Properties**, set *Assignment required?* to *Yes* to limit access to assigned users and groups. When left at *No*, everyone in the tenant can sign in as a normal user.

Roles are re-read at every sign-in, so removing someone from the admin group takes effect the next time they sign in. Tenant roles such as Global Administrator do **not** grant admin rights in this app.

### 4a. Linux (systemd + Caddy)

```sh
sudo useradd --system --home /var/lib/udgifter --shell /usr/sbin/nologin udgifter
sudo mkdir -p /opt/udgifter /var/lib/udgifter /etc/udgifter
sudo chown udgifter:udgifter /var/lib/udgifter
sudo cp expenses create-user /opt/udgifter/ && sudo cp -r site /opt/udgifter/
sudo nano /etc/udgifter/env            # settings from step 2
sudo chown root:udgifter /etc/udgifter/env && sudo chmod 640 /etc/udgifter/env
```

`/etc/systemd/system/udgifter.service`:

```ini
[Unit]
Description=Udgifter expense app
After=network-online.target
Wants=network-online.target

[Service]
User=udgifter
Group=udgifter
WorkingDirectory=/opt/udgifter
EnvironmentFile=/etc/udgifter/env
ExecStart=/opt/udgifter/expenses
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/lib/udgifter

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload && sudo systemctl enable --now udgifter
journalctl -u udgifter -f
```

`/etc/caddy/Caddyfile`:

```caddy
udgifter.example.dk {
    encode zstd gzip
    request_body {
        max_size 64MB
    }
    reverse_proxy 127.0.0.1:3000
}
```

Caddy fetches a Let's Encrypt certificate automatically. This needs public DNS pointing at the server and ports 80/443 open. For an internal-only server, use `tls internal` or your company certificate (`tls cert.pem key.pem`) instead.

### 4b. Windows Server (service + Caddy)

The binary has no built-in Windows service support, so run it with a service wrapper such as [NSSM](https://nssm.cc) or [WinSW](https://github.com/winsw/winsw). With NSSM, from an elevated prompt:

```powershell
New-Item -ItemType Directory C:\Udgifter\data -Force
# copy expenses.exe, create-user.exe and site\ to C:\Udgifter, and put the settings from step 2 in C:\Udgifter\.env
nssm install Udgifter C:\Udgifter\expenses.exe
nssm set Udgifter AppDirectory C:\Udgifter
nssm set Udgifter AppStdout C:\Udgifter\logs\udgifter.log
nssm set Udgifter AppStderr C:\Udgifter\logs\udgifter.log
nssm set Udgifter AppRotateFiles 1
nssm set Udgifter ObjectName "NT AUTHORITY\LocalService"
nssm start Udgifter
```

Grant the service account write access to `C:\Udgifter\data` and `C:\Udgifter\logs`, and remove access to `C:\Udgifter\.env` for everyone except that account and Administrators:

```powershell
icacls C:\Udgifter\data /grant "LOCAL SERVICE:(OI)(CI)M"
icacls C:\Udgifter\logs /grant "LOCAL SERVICE:(OI)(CI)M"
icacls C:\Udgifter\.env /inheritance:r /grant "LOCAL SERVICE:R" /grant "Administrators:F"
```

Run Caddy with the same Caddyfile as above, as its own service (see the [Caddy Windows docs](https://caddyserver.com/docs/running#windows-service)). IIS with URL Rewrite + ARR also works as the reverse proxy. If you use IIS, raise its request size limit to 64 MB and make sure it forwards `X-Forwarded-For`.

### 5. Create local users

`create-user` must use the same `DATA_DIR` (or `DATABASE_URL`) as the server. It prompts for the password (at least 12 characters).

```sh
# Linux
sudo -u udgifter bash -c 'set -a; . /etc/udgifter/env; exec /opt/udgifter/create-user --username admin --role admin'
```

```powershell
# Windows (reads C:\Udgifter\.env)
cd C:\Udgifter; .\create-user.exe --username admin --role admin
```

Microsoft users don't need to be created. They are added automatically at their first sign-in.

### 6. Backups

Everything lives in `DATA_DIR`: `expenses.db` (plus `-wal`/`-shm` while running) and `files/`. Back up a consistent copy of the database and the file folder nightly:

```sh
# Linux (cron, as udgifter); needs the sqlite3 CLI
sqlite3 /var/lib/udgifter/expenses.db "VACUUM INTO '/backup/udgifter/expenses-$(date +%F).db'"
rsync -a /var/lib/udgifter/files/ /backup/udgifter/files/
```

```powershell
# Windows (Task Scheduler); needs sqlite3.exe
sqlite3 C:\Udgifter\data\expenses.db "VACUUM INTO 'D:\Backup\Udgifter\expenses-$(Get-Date -f yyyy-MM-dd).db'"
robocopy C:\Udgifter\data\files D:\Backup\Udgifter\files /E
```

Don't back up the live `expenses.db` by copying the file while the service is running, because the copy can be inconsistent. Files are write-once (named by their SHA-256 hash), so incremental copies are safe.

### 7. Updating

1. Build as in step 1.
2. Stop the service (`systemctl stop udgifter` / `nssm stop Udgifter`).
3. Replace the binaries and the **whole** `site/` folder, because the WASM bundle must match the server binary.
4. Start the service. Database migrations run automatically at startup.

Browsers check `/pkg/*` with the server on every page load (`Cache-Control: no-cache` plus 304 responses), so a deploy takes effect at once. If a browser still shows `LinkError: import object field '__wbindgen_…'` in the console, it has JS and WASM from different builds: make sure `site/` and the binary come from the same build, then hard-refresh.

Take a backup first if the release contains a new file in `migrations/`.

### Checklist after deploy

- `https://<host>/` redirects to the login page, which is shown in Danish.
- "Log ind med Microsoft" completes and returns to the app. If it doesn't, check the redirect URI and the client secret.
- A local user can log in.
- Taking a photo from a phone uploads it.
- The log shows `ECB rates refreshed` shortly after the first start.
