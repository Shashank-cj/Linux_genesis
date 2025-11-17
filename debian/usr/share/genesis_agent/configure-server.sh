#!/bin/bash
sudo rm -f /etc/genesis_agent/config.env



CONFIG_FILE="/etc/genesis_agent/config.env"

# Create config directory
mkdir -p /etc/genesis_agent

# ==========================================================
# Step 0: Check for existing configuration
# ==========================================================
if [ -f "$CONFIG_FILE" ]; then
    echo ""
    echo " Existing configuration found at $CONFIG_FILE"
    read -p "Do you want to reconfigure? [y/N]: " RECONFIRM
    if [[ ! $RECONFIRM =~ ^[Yy]$ ]]; then
        echo "Using existing configuration. Exiting setup."
        exit 0
    fi
    rm -f "$CONFIG_FILE"
    echo " Old configuration removed. Proceeding with new setup..."
fi

# ==========================================================
# Step 1: Detect mode (interactive / non-interactive)
# ==========================================================
if [ -n "$GENESIS_AGENT_CENTRAL_SERVER_URL" ]; then
    # Non-interactive mode (env vars provided)
    CENTRAL_SERVER_URL="$GENESIS_AGENT_CENTRAL_SERVER_URL"
    WS_URL="${GENESIS_AGENT_WEB_SOCKET_URL:-${CENTRAL_SERVER_URL/https:/wss:}}"
    WS_URL="${WS_URL/http:/ws:}"

    echo "Using provided configuration:"
    echo "  Central Server: $CENTRAL_SERVER_URL"
    echo "  WebSocket URL: $WS_URL"
else
    # Interactive mode
    echo "========================================="
    echo "Ubuntu Agent Configuration"
    echo "========================================="
    echo ""
    echo "Enter your central server address:"
    echo ""
    echo "Examples:"
    echo "  192.168.100.13         (will use HTTPS)"
    echo "  monitor.example.com    (will use HTTPS)"
    echo "  localhost:8000         (will use HTTP for development)"
    echo ""
    read -p "Server Address: " SERVER_INPUT

    # Clean input
    SERVER_INPUT="${SERVER_INPUT#http://}"
    SERVER_INPUT="${SERVER_INPUT#https://}"
    SERVER_INPUT="${SERVER_INPUT#ws://}"
    SERVER_INPUT="${SERVER_INPUT#wss://}"

    # Smart protocol detection
    if [[ "$SERVER_INPUT" =~ ^(localhost|127\.0\.0\.1) ]]; then
        PROTOCOL="http"
        WS_PROTOCOL="ws"
        echo ""
        echo "Development server detected - using HTTP"
    else
        PROTOCOL="https"
        WS_PROTOCOL="wss"
        echo ""
        echo "Production server detected - using HTTPS"
    fi

    # Generate URLs
    CENTRAL_SERVER_URL="${PROTOCOL}://${SERVER_INPUT}"
    WS_URL="${WS_PROTOCOL}://${SERVER_INPUT}"

    echo ""
    echo "Generated configuration:"
    echo "  Central Server: $CENTRAL_SERVER_URL"
    echo "  WebSocket URL: $WS_URL"
    echo ""
    read -p "Is this correct? [Y/n]: " CONFIRM

    if [[ $CONFIRM =~ ^[Nn]$ ]]; then
        echo ""
        echo "Manual configuration:"
        read -p "Central Server URL: " CENTRAL_SERVER_URL
        read -p "WebSocket URL: " WS_URL
    fi
fi

# ==========================================================
# Step 2: Create configuration file
# ==========================================================
cat > "$CONFIG_FILE" << CONFIGEOF
# Ubuntu Agent Configuration
# Generated on $(date)

GENESIS_AGENT_CENTRAL_SERVER_URL=$CENTRAL_SERVER_URL
GENESIS_AGENT_WEB_SOCKET_URL=$WS_URL
GENESIS_AGENT_APP_DIR=/var/lib/genesis_agent
GENESIS_AGENT_DB_PATH=/var/lib/genesis_agent/monitoring.db
GENESIS_AGENT_NATS_URL=tls://127.0.0.1:4222
PYTHONPATH=/var/lib/genesis_agent/agent_collector
RUST_LOG=info
CONFIGEOF

chmod 644 "$CONFIG_FILE"
chown root:root "$CONFIG_FILE"

# ==========================================================
# Step 3: Display final summary
# ==========================================================
echo ""
echo "========================================="
echo "Configuration saved to $CONFIG_FILE"
echo "========================================="
echo ""
echo "Final configuration:"
echo "  Central Server: $CENTRAL_SERVER_URL"
echo "  WebSocket URL:  $WS_URL"
echo ""
