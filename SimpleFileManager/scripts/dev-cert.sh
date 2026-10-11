#!/usr/bin/env bash
# Local self-signed certificate for the Vite dev server.
#
# Its only job is to make http://<lan-ip>:15173 into an https origin, which is
# the minimum for registering a service worker while developing the PWA shell.
# It is NOT trusted by anything -- browsers will show a warning, and a service
# worker may or may not register depending on the device. Real deployment is
# expected to use a real certificate (or its own self-signed pair); nothing here
# is required for production.
#
# Regenerate if the LAN IP changes:
#   ./scripts/dev-cert.sh
set -euo pipefail

cd "$(dirname "$0")/.."
OUT_DIR="frontend"
KEY="$OUT_DIR/dev-key.pem"
CRT="$OUT_DIR/dev-cert.pem"

mkdir -p "$OUT_DIR"

# SAN has to carry the address you actually type, otherwise the name check
# fails even though the certificate itself is fine.
IPS=$(hostname -I 2>/dev/null | tr ' ' '\n' | grep -E '^[0-9]+\.' | paste -sd, -)
[ -n "$IPS" ] || IPS="127.0.0.1"

echo "generating self-signed cert for: $IPS, localhost"

openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "$KEY" -out "$CRT" -days 365 \
    -subj "/CN=SimpleFileManager Dev" \
    -addext "subjectAltName=IP:$IPS,IP:127.0.0.1,DNS:localhost" \
    2>/dev/null

chmod 600 "$KEY"
echo "wrote $CRT and $KEY"