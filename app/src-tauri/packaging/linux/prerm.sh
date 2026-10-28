#!/usr/bin/env bash
# ==============================================================================
# Yusr Debian/Ubuntu prerm script
# Gracefully stops and disables systemd service before removal
# ==============================================================================
set -e

if command -v systemctl >/dev/null 2>&1; then
    if systemctl is-active --quiet yusr.service 2>/dev/null; then
        echo "[Yusr] Stopping yusr.service..."
        systemctl stop yusr.service || true
    fi
    echo "[Yusr] Disabling yusr.service..."
    systemctl disable yusr.service 2>/dev/null || true
fi

rm -f /etc/xdg/autostart/yusr.desktop || true

exit 0
