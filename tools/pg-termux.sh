#!/data/data/com.termux/files/usr/bin/bash
# Runs Yusr's PostgreSQL natively in Termux. Run it from a plain Termux session,
# never from proot-distro: PostgreSQL under proot's ptrace hangs (initdb freezes at
# bootstrap, queries stall). Apps inside proot still reach it at 127.0.0.1:5432.
#
#   bash tools/pg-termux.sh [start|stop|status]     (default: start)
#
# Env overrides: YUSR_DB_NAME, YUSR_DB_USER, YUSR_DB_PASSWORD, PGDATA, PGPORT.
set -euo pipefail

die() { echo "pg-termux: $*" >&2; exit 1; }

[ -n "${PREFIX:-}" ] && [ -x "$PREFIX/bin/pg_ctl" ] || die "run inside Termux with 'pkg install postgresql' done"
grep -qE '^TracerPid:\s+0$' /proc/self/status || die "running under proot/ptrace; open a plain Termux session instead"

DB_NAME="${YUSR_DB_NAME:-yusr}"
DB_USER="${YUSR_DB_USER:-yusr}"
DB_PASSWORD="${YUSR_DB_PASSWORD:-yusr}"
PGDATA="${PGDATA:-$PREFIX/var/lib/postgresql}"
PGPORT="${PGPORT:-5432}"
# Bounds every server call so a stuck postgres fails loudly instead of hanging the shell.
STEP_TIMEOUT=60

pg_ctl_() { timeout "$STEP_TIMEOUT" pg_ctl -D "$PGDATA" "$@"; }
psql_() { timeout "$STEP_TIMEOUT" psql -X -v ON_ERROR_STOP=1 -h 127.0.0.1 -p "$PGPORT" -d postgres "$@"; }

start() {
    if [ ! -f "$PGDATA/PG_VERSION" ]; then
        echo "initialising cluster in $PGDATA"
        mkdir -p "$PGDATA"
        timeout "$STEP_TIMEOUT" initdb -D "$PGDATA" -E UTF8 --locale=C --auth=trust
    fi
    if pg_ctl_ status >/dev/null 2>&1; then
        echo "postgres already running"
    else
        pg_ctl_ -l "$PGDATA/logfile" -o "-p $PGPORT -c listen_addresses=127.0.0.1" -w -t "$STEP_TIMEOUT" start \
            || { tail -20 "$PGDATA/logfile" >&2; die "start failed (log above)"; }
    fi

    # The cluster superuser is the Termux user; one -c per statement (CREATE DATABASE
    # cannot run inside the implicit transaction of a multi-statement -c).
    if [ "$(psql_ -tAc "SELECT 1 FROM pg_roles WHERE rolname = '$DB_USER'")" != 1 ]; then
        psql_ -c "CREATE ROLE \"$DB_USER\" LOGIN CREATEDB PASSWORD '$DB_PASSWORD'"
    fi
    if [ "$(psql_ -tAc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'")" != 1 ]; then
        psql_ -c "CREATE DATABASE \"$DB_NAME\" OWNER \"$DB_USER\" ENCODING 'UTF8'"
    fi

    echo "ready:"
    echo "  DATABASE_URL=postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:$PGPORT/$DB_NAME"
    echo "  YUSR_TEST_DATABASE_URL=postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:$PGPORT/postgres"
}

case "${1:-start}" in
    start) start ;;
    stop) pg_ctl_ -m fast stop ;;
    status) pg_ctl_ status ;;
    *) die "usage: $0 [start|stop|status]" ;;
esac
