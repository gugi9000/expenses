#!/usr/bin/env bash
# Builds the release and installs it on this machine; restarts the service only if the output changed.
# Run as a normal user with sudo rights (not as root, so cargo uses your toolchain).
set -euo pipefail

DEST=${DEST:-/opt/expenses}
SERVICE=${SERVICE:-expenses}
ADDR=${ADDR:-127.0.0.1:3000}

cd "$(dirname "$0")"

cargo leptos build --release
cargo build --release --features ssr --bin create-user

changed=false
for bin in expenses create-user; do
    cmp -s "target/release/$bin" "$DEST/$bin" || changed=true
done
diff -rq target/site "$DEST/site" >/dev/null 2>&1 || changed=true

if [ "$changed" = false ]; then
    echo "Nothing changed; $SERVICE left running."
    exit 0
fi

echo "Stopping $SERVICE"
sudo systemctl stop "$SERVICE"
sudo install -m 755 target/release/expenses target/release/create-user "$DEST/"
sudo rm -rf "$DEST/site"
sudo cp -r target/site "$DEST/site"
sudo chmod -R a+rX "$DEST/site"
echo "Starting $SERVICE"
sudo systemctl start "$SERVICE"

for _ in $(seq 1 20); do
    if version=$(curl -fsS "http://$ADDR/version" 2>/dev/null); then
        echo "Deployed $version"
        exit 0
    fi
    sleep 0.5
done
echo "$SERVICE did not respond on $ADDR; recent log:" >&2
sudo journalctl -u "$SERVICE" -n 30 --no-pager >&2
exit 1
