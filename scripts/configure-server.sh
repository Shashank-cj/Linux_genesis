
set -e

CONFIG_DIR="/etc/genesis_agent"
CONFIG_FILE="$CONFIG_DIR/config.env"

mkdir -p "$CONFIG_DIR"

# ==========================================================
# Step 0: Check for existing configuration
# ==========================================================
if [ -f "$CONFIG_FILE" ]; then
    echo ""
    echo "Existing configuration found at $CONFIG_FILE"
    read -p "Do you want to reconfigure? [y/N]: " RECONFIRM
    if [[ ! "$RECONFIRM" =~ ^[Yy]$ ]]; then
        echo "Using existing configuration. Exiting setup."
        exit 0
    fi
    rm -f "$CONFIG_FILE"
    echo "Old configuration removed. Proceeding with new setup..."
fi

# ==========================================================
# Step 1: Detect mode (non-interactive / interactive)
# ==========================================================
if [ -n "$GENESIS_AGENT_CENTRAL_SERVER_URL" ]; then
    # Non-interactive (CI / automation)
    CENTRAL_SERVER_URL="$GENESIS_AGENT_CENTRAL_SERVER_URL"

    WS_URL="${GENESIS_AGENT_WEB_SOCKET_URL:-${CENTRAL_SERVER_URL/https:/wss:}}"
    WS_URL="${WS_URL/http:/ws:}"

else
    # Interactive mode (ONLY server info)
    echo "========================================="
    echo " Genesis Agent Configuration"
    echo "========================================="
    echo ""
    echo "Enter your central server address"
    echo "Examples:"
    echo "  192.168.100.13"
    echo "  monitor.example.com"
    echo "  localhost:8000"
    echo ""

    read -p "Server Address: " SERVER_INPUT

    # Clean protocol
    SERVER_INPUT="${SERVER_INPUT#http://}"
    SERVER_INPUT="${SERVER_INPUT#https://}"
    SERVER_INPUT="${SERVER_INPUT#ws://}"
    SERVER_INPUT="${SERVER_INPUT#wss://}"

    MACHINE_UUID="$(tr -d '\n' < /etc/machine-id)"

    if [[ "$SERVER_INPUT" =~ ^(localhost|127\.0\.0\.1) ]]; then
        PROTOCOL="http"
        WS_PROTOCOL="ws"
        echo "Development server detected"
    else
        PROTOCOL="https"
        WS_PROTOCOL="wss"
        echo "Production server detected"
    fi

    CENTRAL_SERVER_URL="${PROTOCOL}://${SERVER_INPUT}"
    WS_URL="${WS_PROTOCOL}://${SERVER_INPUT}"

    echo ""
    echo "Generated configuration:"
    echo "  Central Server : $CENTRAL_SERVER_URL"
    echo "  WebSocket URL  : $WS_URL"
    echo ""

    read -p "Is this correct? [Y/n]: " CONFIRM
    if [[ "$CONFIRM" =~ ^[Nn]$ ]]; then
        read -p "Central Server URL: " CENTRAL_SERVER_URL
        read -p "WebSocket URL: " WS_URL
    fi
fi

# ==========================================================
# Step 2: Write configuration file
# (NO validation, NO logic — Rust handles safety)
# ==========================================================
cat > "$CONFIG_FILE" <<EOF
# ==========================================================
# Genesis Agent Configuration
# Generated on $(date)
# ==========================================================

# Server endpoints
GENESIS_AGENT_CENTRAL_SERVER_URL=$CENTRAL_SERVER_URL
GENESIS_AGENT_WEB_SOCKET_URL=$WS_URL

# Paths
GENESIS_AGENT_APP_DIR=/var/lib/genesis_agent
GENESIS_AGENT_DB_PATH=/var/lib/genesis_agent/monitoring.db
GENESIS_AGENT_DB_KEY=$MACHINE_UUID
GENESIS_AGENT_PYTHON_VENV=/opt/genesis_agent/venv

# Monitoring configuration
GENESIS_AGENT_MONITOR_INTERVAL_SEC=2
GENESIS_AGENT_MONITOR_INTERVAL=5

# NATS
GENESIS_AGENT_NATS_URL=tls://127.0.0.1:4222

# Logging
RUST_LOG=info
EOF

chmod 644 "$CONFIG_FILE"
chown root:root "$CONFIG_FILE"

# ==========================================================
# Step 3: Final summary
# ==========================================================
echo ""
echo "========================================="
echo " Configuration saved successfully"
echo "========================================="
echo ""
echo "Central Server : $CENTRAL_SERVER_URL"
echo "WebSocket URL  : $WS_URL"
echo ""
echo "Monitoring:"
echo "  Interval Sec : 2  (editable in config)"
echo "  Batch Size  : 5  (editable in config)"
echo ""
echo "Config file   : $CONFIG_FILE"
echo ""
echo "Restart services to apply changes:"
echo "  sudo systemctl restart genesis-agent-bridge"
echo "  sudo systemctl restart genesis-agent-collector"
echo ""
