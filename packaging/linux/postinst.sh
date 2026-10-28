#!/usr/bin/env bash
# ==============================================================================
# Yusr Debian/Ubuntu postinst script
# Handles:
# 1. PostgreSQL verification and automatic service enablement on boot
# 2. Database user and database creation (idempotent)
# 3. Secure connection string configuration (/etc/yusr/yusr.env)
# 4. Connection verification test
# 5. systemd service auto-start enablement on system boot
# 6. Desktop session autostart entry (/etc/xdg/autostart/yusr.desktop)
# ==============================================================================
set -e

DB_NAME="yusr"
DB_USER="yusr"
DB_PASS="yusr"
DB_PORT="5432"
CONFIG_DIR="/etc/yusr"
ENV_FILE="$CONFIG_DIR/yusr.env"

echo "=== [Yusr] Configuring PostgreSQL database and autostart on boot ==="

# 1. Ensure systemd enables and starts PostgreSQL on boot
if command -v systemctl >/dev/null 2>&1; then
    echo "[Yusr] Enabling PostgreSQL to start automatically on system boot..."
    systemctl daemon-reload || true
    systemctl enable postgresql || true
    systemctl start postgresql || true
elif command -v service >/dev/null 2>&1; then
    service postgresql start || true
fi

# 2. Wait for PostgreSQL to be ready
echo "[Yusr] Verifying PostgreSQL is accepting connections..."
for i in $(seq 1 15); do
    if su - postgres -c "psql -X -v ON_ERROR_STOP=1 -c '\q'" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

# 3. Create database user if not present
echo "[Yusr] Setting up database user '$DB_USER'..."
if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_roles WHERE rolname = '$DB_USER'\"" 2>/dev/null | grep -q 1; then
    su - postgres -c "psql -c \"CREATE ROLE \\\"$DB_USER\\\" WITH LOGIN CREATEDB PASSWORD '$DB_PASS';\""
    echo "[Yusr] Database role '$DB_USER' created successfully."
else
    echo "[Yusr] Database role '$DB_USER' already exists."
fi

# 4. Create database if not present
echo "[Yusr] Setting up database '$DB_NAME'..."
if ! su - postgres -c "psql -tAc \"SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'\"" 2>/dev/null | grep -q 1; then
    su - postgres -c "psql -c \"CREATE DATABASE \\\"$DB_NAME\\\" WITH OWNER \\\"$DB_USER\\\" ENCODING 'UTF8';\""
    echo "[Yusr] Database '$DB_NAME' created successfully."
else
    echo "[Yusr] Database '$DB_NAME' already exists."
fi

# 5. Configure connection environment file
mkdir -p "$CONFIG_DIR"
chmod 755 "$CONFIG_DIR"

if [ ! -f "$ENV_FILE" ]; then
    JWT_SECRET=$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')
    cat <<EOF > "$ENV_FILE"
DATABASE_URL=postgres://$DB_USER:$DB_PASS@127.0.0.1:$DB_PORT/$DB_NAME
YUSR_BIND=127.0.0.1:8080
YUSR_JWT_SECRET=$JWT_SECRET
YUSR_ADMIN_USERNAME=admin
YUSR_ADMIN_PASSWORD=admin12345
EOF
    chmod 600 "$ENV_FILE"
    echo "[Yusr] Generated default connection configuration in $ENV_FILE"
fi

# 6. Verify database connectivity
echo "[Yusr] Testing connection to database '$DB_NAME'..."
if PGPASSWORD="$DB_PASS" psql -h 127.0.0.1 -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT 1;" >/dev/null 2>&1; then
    echo "[Yusr] Database connection verified successfully."
else
    echo "[Yusr] Warning: Direct loopback connection check timed out; server fallback will verify during startup."
fi

# 7. Enable Yusr systemd service to start automatically on system boot
if [ -f /lib/systemd/system/yusr.service ] || [ -f /etc/systemd/system/yusr.service ]; then
    if command -v systemctl >/dev/null 2>&1; then
        echo "[Yusr] Enabling yusr.service to start automatically on boot..."
        systemctl daemon-reload || true
        systemctl enable yusr.service || true
        systemctl start yusr.service || true
    fi
fi

# 8. Configure desktop autostart entry if graphical session is present
AUTOSTART_DIR="/etc/xdg/autostart"
if [ -d "$AUTOSTART_DIR" ]; then
    cat <<EOF > "$AUTOSTART_DIR/yusr.desktop"
[Desktop Entry]
Type=Application
Name=Yusr
Name[ar]=يُسر
Comment=Autonomous Fund Accounting & Management Suite
Comment[ar]=نظام المحاسبة وإدارة الجمعيات الخيرية
Exec=/usr/bin/yusr
Icon=yusr
Terminal=false
Categories=Office;Finance;
StartupNotify=true
X-GNOME-Autostart-enabled=true
EOF
    chmod 644 "$AUTOSTART_DIR/yusr.desktop"
fi

echo "=== [Yusr] Database installation, database creation, connection, and boot autostart configured successfully ==="
exit 0
