#!/bin/bash

if [ "$EUID" -ne 0 ]; then 
    echo "Please run with sudo"
    exit 1
fi

echo "Reconfiguring genesis_agent..."
echo ""

# Run configuration script
/opt/genesis_agent/configure-server.sh

# Restart services to apply new config
echo ""
echo "Restarting services to apply new configuration..."
systemctl restart genesis-agent-bridge
sleep 2
systemctl restart genesis-agent-collector

echo ""
echo "Configuration updated and services restarted"
echo ""
echo "Check status: sudo systemctl status genesis-agent-bridge genesis-agent-collector"
