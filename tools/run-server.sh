#!/usr/bin/env bash
# Builds the web UI if needed and runs yusr-server against the Termux PostgreSQL
# started by tools/pg-termux.sh. Works from proot-distro Ubuntu or Termux.
#
#   YUSR_ADMIN_PASSWORD='...' bash tools/run-server.sh     (first run creates the admin)
set -euo pipefail
cd "$(dirname "$0")/.."

# Defaults match tools/pg-termux.sh; anything already exported wins.
export DATABASE_URL="${DATABASE_URL:-postgres://yusr:yusr@127.0.0.1:5432/yusr}"
export YUSR_BIND="${YUSR_BIND:-127.0.0.1:8080}"
export YUSR_UI_DIR="${YUSR_UI_DIR:-$PWD/web/dist}"
export YUSR_ADMIN_USERNAME="${YUSR_ADMIN_USERNAME:-admin}"

# A stable secret keeps sessions valid across restarts; generated once, never committed.
SECRET_FILE=.yusr-jwt-secret
if [ -z "${YUSR_JWT_SECRET:-}" ]; then
    [ -s "$SECRET_FILE" ] || { head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$SECRET_FILE"; chmod 600 "$SECRET_FILE"; }
    YUSR_JWT_SECRET="$(< "$SECRET_FILE")"
    export YUSR_JWT_SECRET
fi

# Opt-in local e-invoicing with a self-signed key (.zatca-dev/); ZATCA rejects these invoices.
if [ "${YUSR_ZATCA_DEV:-}" = 1 ] && [ -z "${YUSR_ZATCA_KEY_FILE:-}" ]; then
    if [ ! -s .zatca-dev/key.pem ]; then
        mkdir -p .zatca-dev && chmod 700 .zatca-dev
        openssl ecparam -name secp256k1 -genkey -noout -out .zatca-dev/key.pem && chmod 600 .zatca-dev/key.pem
        openssl req -new -x509 -key .zatca-dev/key.pem -out .zatca-dev/cert.pem -days 365 \
            -subj "/C=SA/O=Yusr Development/OU=Local test only/CN=yusr-dev"
    fi
    export YUSR_ZATCA_KEY_FILE="$PWD/.zatca-dev/key.pem" YUSR_ZATCA_CERT_FILE="$PWD/.zatca-dev/cert.pem"
    echo "run-server: e-invoicing uses the LOCAL TEST certificate in .zatca-dev (not valid for ZATCA)" >&2
fi

# Fail fast instead of hanging when the database is down (the old proot symptom).
host_port="${DATABASE_URL##*@}"; host_port="${host_port%%/*}"
if ! timeout 5 bash -c "exec 3<>/dev/tcp/${host_port%:*}/${host_port##*:}" 2>/dev/null; then
    echo "run-server: PostgreSQL is not reachable at $host_port; start it in Termux: bash tools/pg-termux.sh" >&2
    exit 1
fi

if [ ! -f "$YUSR_UI_DIR/index.html" ]; then
    (cd web && { [ -d node_modules ] || npm install --no-audit --no-fund; } && npm run build)
fi

export PATH="$HOME/.cargo/bin:$PATH"
exec cargo run --release -p yusr-server
