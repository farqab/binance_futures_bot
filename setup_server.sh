#!/bin/bash
set -e
sudo systemctl stop binance-web-server.service 2>/dev/null || true
sudo systemctl disable binance-web-server.service 2>/dev/null || true
sudo rm -f /etc/systemd/system/binance-web-server.service

cat << 'EOF' > /tmp/bot-dashboard.service
[Unit]
Description=Bot Live Web Dashboard & Control API
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/binance_futures_bot
ExecStart=/usr/bin/python3 /home/ubuntu/binance_futures_bot/web_server.py
Restart=always

[Install]
WantedBy=multi-user.target
EOF

sudo mv /tmp/bot-dashboard.service /etc/systemd/system/bot-dashboard.service
sudo systemctl daemon-reload
sudo systemctl restart bot-dashboard.service
echo "DASHBOARD_UPDATED"
