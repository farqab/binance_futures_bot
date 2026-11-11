#!/usr/bin/env bash
set -euo pipefail

echo "=================================================================="
echo "🚀 Binance Futures Rust Bot v5.0 — AWS EC2 Otomatik Kurulum (Linux)"
echo "=================================================================="

# 1. Swap Alanı Oluştur (1GB RAM t3.micro için derleme esnasında OOM koruması)
if [ ! -f /swapfile ]; then
    echo "💾 2GB Swap hafıza alanı oluşturuluyor..."
    sudo fallocate -l 2G /swapfile 2>/dev/null || sudo dd if=/dev/zero of=/swapfile bs=1M count=2048
    sudo chmod 600 /swapfile
    sudo mkswap /swapfile
    sudo swapon /swapfile 2>/dev/null || true
fi

# 2. Sistem paketlerini ve derleme araçlarını kur (Ubuntu/Debian & Amazon Linux uyumlu)
if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update -y
    sudo apt-get install -y build-essential pkg-config libssl-dev curl git ca-certificates
elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y gcc openssl-devel pkgconfig curl git ca-certificates
elif command -v yum >/dev/null 2>&1; then
    sudo yum install -y gcc openssl-devel pkgconfig curl git ca-certificates
fi

# 2. Rust Toolchain kurulu değilse kur
if ! command -v cargo >/dev/null 2>&1; then
    echo "📦 Rust (rustup) kuruluyor..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# 3. Release (Maksimum Hız / LTO) modunda derle
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_DIR"
echo "⚙️ Rust binary --release modunda derleniyor..."
"$HOME/.cargo/bin/cargo" build --release

# 4. API Anahtar Dosyası Şablonunu Kontrol Et
if [ ! -f "$PROJECT_DIR/testnet_keys.json" ]; then
    cat > "$PROJECT_DIR/testnet_keys.json" <<EOF
{
  "api_key": "BURAYA_BINANCE_MAINNET_API_KEY_YAZIN",
  "api_secret": "BURAYA_BINANCE_MAINNET_SECRET_KEY_YAZIN"
}
EOF
    chmod 600 "$PROJECT_DIR/testnet_keys.json"
    echo "⚠️ testnet_keys.json oluşturuldu. Gerçek işlem için API ve Secret Key'inizi içine yazın."
fi

# 5. 7/24 Otomatik Yeniden Başlatmalı (systemd) Servis Kurulumu
SERVICE_FILE="/etc/systemd/system/binance-futures-bot.service"
USER_NAME="$(whoami)"

sudo bash -c "cat > $SERVICE_FILE" << 'EOF'
[Unit]
Description=Binance Futures Multi-Coin Rust Quant Bot v5.0
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=/home/ubuntu/binance_futures_bot
ExecStart=/home/ubuntu/binance_futures_bot/target/release/binance_futures_bot
Restart=always
RestartSec=3
Environment=RUST_BACKTRACE=1

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable binance-futures-bot.service
sudo systemctl restart binance-futures-bot.service

echo "✅ Kurulum Tamamlandı!"
echo "📊 Canlı logları izlemek için: sudo journalctl -u binance-futures-bot -f"
echo "🧪 Backtest çalıştırmak için : ./target/release/binance_futures_bot --backtest"
