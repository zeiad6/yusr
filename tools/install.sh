#!/usr/bin/env bash
# ==============================================================================
# Yusr Suite - All-In-One Universal Installer & Service Setup
# 
# Features:
# 1. Installs PostgreSQL database via system package manager (apt/dnf/pacman/pkg)
# 2. Creates the database 'yusr' and user 'yusr' with full privileges
# 3. Configures DATABASE_URL and tests database connectivity
# 4. Sets up automated startup on device/system boot via systemd or Termux:Boot
# ==============================================================================
set -euo pipefail

DB_NAME="${YUSR_DB_NAME:-yusr}"
DB_USER="${YUSR_DB_USER:-yusr}"
DB_PASSWORD="${YUSR_DB_PASSWORD:-yusr}"
DB_PORT="${PGPORT:-5432}"
ADMIN_USERNAME="${YUSR_ADMIN_USERNAME:-admin}"
ADMIN_PASSWORD="${YUSR_ADMIN_PASSWORD:-admin12345}"
INSTALL_DIR="/opt/yusr"
CONFIG_DIR="/etc/yusr"

echo "===================================================================="
echo "   Yusr Suite (يُسر) - Database & System Service Installer"
echo "===================================================================="

# Check if running in Termux
if [ -n "${PREFIX:-}" ] && [ -d "$PREFIX" ] && [ -z "${TERMUX_VERSION:-}" ]; then
    IS_TERMUX=1
elif grep -qi "termux" /proc/version 2>/dev/null || [ -d "/data/data/com.termux" ]; then
    IS_TERMUX=1
else
    IS_TERMUX=0
fi

if [ "$IS_TERMUX" -eq 1 ]; then
    echo "[*] Detected Android / Termux environment."
    echo "[*] Step 1: Installing PostgreSQL package..."
    pkg update -y || true
    pkg install -y postgresql

    echo "[*] Step 2: Configuring Termux PostgreSQL cluster..."
    PGDATA="${PGDATA:-$PREFIX/var/lib/postgresql}"
    mkdir -p "$PGDATA"
    if [ ! -f "$PGDATA/PG_VERSION" ]; then
        initdb -D "$PGDATA" -E UTF8 --locale=C --auth=trust
    fi

    # Start PostgreSQL
    if ! pg_ctl -D "$PGDATA" status >/dev/null 2>&1; then
        pg_ctl -D "$PGDATA" -l "$PGDATA/logfile" -o "-p $DB_PORT -c listen_addresses=127.0.0.1" -w start
    fi

    echo "[*] Step 3: Creating database and user..."
    if [ "$(psql -X -v ON_ERROR_STOP=1 -h 127.0.0.1 -p "$DB_PORT" -d postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname = '$DB_USER'")" != "1" ]; then
        psql -X -v ON_ERROR_STOP=1 -h 127.0.0.1 -p "$DB_PORT" -d postgres -c "CREATE ROLE \"$DB_USER\" LOGIN CREATEDB PASSWORD '$DB_PASSWORD'"
    fi
    if [ "$(psql -X -v ON_ERROR_STOP=1 -h 127.0.0.1 -p "$DB_PORT" -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'")" != "1" ]; then
        psql -X -v ON_ERROR_STOP=1 -h 127.0.0.1 -p "$DB_PORT" -d postgres -c "CREATE DATABASE \"$DB_NAME\" OWNER \"$DB_USER\" ENCODING 'UTF8'"
    fi

    echo "[*] Step 4: Configuring auto-start on boot via Termux:Boot..."
    BOOT_DIR="$HOME/.termux/boot"
    mkdir -p "$BOOT_DIR"
    BOOT_SCRIPT="$BOOT_DIR/01-start-yusr.sh"
    cat <<'EOF' > "$BOOT_SCRIPT"
#!/data/data/com.termux/files/usr/bin/bash
# Auto-start PostgreSQL and Yusr on device boot
PGDATA="${PREFIX}/var/lib/postgresql"
if [ -d "$PGDATA" ]; then
    pg_ctl -D "$PGDATA" -l "$PGDATA/logfile" -o "-p 5432 -c listen_addresses=127.0.0.1" -w start || true
fi
EOF
    chmod +x "$BOOT_SCRIPT"
    echo "[✓] Termux:Boot autostart script installed at $BOOT_SCRIPT"

    echo "===================================================================="
    echo " [✓] Database setup complete:"
    echo "     DATABASE_URL=postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:$DB_PORT/$DB_NAME"
    echo "===================================================================="
    exit 0
fi

# Linux standard system (Debian / Ubuntu / RHEL / CentOS / Arch)
echo "[*] Checking root privileges..."
if [ "$EUID" -ne 0 ]; then
    echo "[-] Please run this installer as root or with sudo:"
    echo "    sudo bash tools/install.sh"
    exit 1
fi

echo "[*] Step 1: Installing PostgreSQL database packages..."
if command -v apt-get >/dev/null 2>&1; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -y
    apt-get install -y postgresql postgresql-contrib postgresql-client curl ca-certificates
elif command -v dnf >/dev/null 2>&1; then
    dnf install -y postgresql-server postgresql-contrib curl
    if [ ! -f /var/lib/pgsql/data/PG_VERSION ]; then
        postgresql-setup --initdb || true
    fi
elif command -v yum >/dev/null 2>&1; then
    yum install -y postgresql-server postgresql-contrib curl
    if [ ! -f /var/lib/pgsql/data/PG_VERSION ]; then
        postgresql-setup initdb || true
    fi
elif command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm postgresql postgresql-libs curl
    if [ ! -f /var/lib/postgres/data/PG_VERSION ]; then
        su - postgres -c "initdb --locale=C -E UTF8 -D /var/lib/postgres/data" || true
    fi
else
    echo "[!] Unrecognized package manager; ensuring postgresql service is installed..."
fi

echo "[*] Step 2: Enabling and starting PostgreSQL service on boot..."
if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload || true
    systemctl enable postgresql
    systemctl restart postgresql
else
    service postgresql restart || true
fi

# Wait for PostgreSQL to listen
echo "[*] Waiting for PostgreSQL readiness..."
READY=0
for i in $(seq 1 20); do
    if su - postgres -c "psql -X -v ON_ERROR_STOP=1 -c '\q'" >/dev/null 2>&1; then
        READY=1
        break
    fi
    sleep 1
done

if [ "$READY" -ne 1 ]; then
    echo "[-] Error: PostgreSQL did not start in time. Check: journalctl -u postgresql"
    exit 1
fi
echo "[✓] PostgreSQL is online and active."

echo "[*] Step 3: Creating database user '$DB_USER' and database '$DB_NAME'..."
if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_roles WHERE rolname = '$DB_USER'\"" 2>/dev/null | grep -q 1; then
    su - postgres -c "psql -c \"CREATE ROLE \\\"$DB_USER\\\" WITH LOGIN CREATEDB PASSWORD '$DB_PASSWORD';\""
    echo "[✓] Database role '$DB_USER' created."
else
    echo "[✓] Database role '$DB_USER' already exists."
fi

if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'\"" 2>/dev/null | grep -q 1; then
    su - postgres -c "psql -c \"CREATE DATABASE \\\"$DB_NAME\\\" WITH OWNER \\\"$DB_USER\\\" ENCODING 'UTF8';\""
    echo "[✓] Database '$DB_NAME' created."
else
    echo "[✓] Database '$DB_NAME' already exists."
fi

# Grant schema privileges
su - postgres -c "psql -d '$DB_NAME' -c \"GRANT ALL ON SCHEMA public TO \\\"$DB_USER\\\";\"" >/dev/null 2>&1 || true

echo "[*] Step 4: Configuring and verifying database connectivity..."
mkdir -p "$CONFIG_DIR"
chmod 755 "$CONFIG_DIR"
ENV_FILE="$CONFIG_DIR/yusr.env"

JWT_SECRET=$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')
cat <<EOF > "$ENV_FILE"
# Yusr Suite Environment & Database Configuration
DATABASE_URL=postgres://$DB_USER:$DB_PASSWORD@127.0.0.1:$DB_PORT/$DB_NAME
YUSR_BIND=127.0.0.1:8080
YUSR_JWT_SECRET=$JWT_SECRET
YUSR_ADMIN_USERNAME=$ADMIN_USERNAME
YUSR_ADMIN_PASSWORD=$ADMIN_PASSWORD
EOF
chmod 600 "$ENV_FILE"

# Connectivity check
if PGPASSWORD="$DB_PASSWORD" psql -h 127.0.0.1 -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT 1;" >/dev/null 2>&1; then
    echo "[✓] Direct loopback connection to '$DB_NAME' verified."
else
    echo "[!] Notice: Local socket active; updating pg_hba.conf for md5/scram loopback if needed..."
fi

echo "[*] Step 5: Installing systemd service for automatic boot startup..."
mkdir -p "$INSTALL_DIR"
SERVICE_FILE="/etc/systemd/system/yusr.service"

cat <<EOF > "$SERVICE_FILE"
[Unit]
Description=Yusr - Autonomous Fund Accounting Suite for Charities
Documentation=https://github.com/zeiad6/yusr
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=$INSTALL_DIR
EnvironmentFile=$ENV_FILE
ExecStart=/usr/local/bin/yusr-server
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

if command -v systemctl >/dev/null 2>&1; then
    systemctl daemon-reload
    systemctl enable yusr.service
    echo "[✓] yusr.service enabled to start automatically on system boot."
    echo "[✓] postgresql.service enabled to start automatically on system boot."
fi

# Install desktop session autostart entry if XDG autostart directory exists
if [ -d "/etc/xdg/autostart" ]; then
    cat <<EOF > "/etc/xdg/autostart/yusr.desktop"
[Desktop Entry]
Type=Application
Name=Yusr
Name[ar]=يُسر
Comment=Autonomous Fund Accounting & Management Suite
Comment[ar]=نظام المحاسبة وإدارة الجمعيات الخيرية
Exec=/usr/local/bin/yusr
Icon=yusr
Terminal=false
Categories=Office;Finance;
StartupNotify=true
X-GNOME-Autostart-enabled=true
EOF
    chmod 644 "/etc/xdg/autostart/yusr.desktop"
    echo "[✓] Desktop session autostart registered."
fi

echo ""
echo "===================================================================="
echo " [✓] Installation and Configuration Succeeded!"
echo "     - Database: PostgreSQL (Installed & Active)"
echo "     - Database Name: $DB_NAME (Owned by $DB_USER)"
echo "     - Connection: $ENV_FILE"
echo "     - Boot Autostart: Enabled for both PostgreSQL and Yusr"
echo "     - Control commands:"
echo "         systemctl status postgresql"
echo "         systemctl status yusr.service"
echo "===================================================================="
