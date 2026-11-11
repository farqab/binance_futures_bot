# 🛡️ Binance Futures Multi-Coin Rust Quant Trading Bot (v5.0 Enterprise)

Production-grade, ultra-low-latency algorithmic quantitative trading system built in **Rust** for **Binance USDT-Margined Perpetual Futures**, orchestrated with an institutional-grade **HTML5/TradingView Lightweight Charts** real-time mission control terminal.

---

## 🏛️ Core Architecture & Design

* **Zero-Garbage-Collection Micro-Latency Engine (Rust):** Multiplexed WebSockets tracking 13 major pairs simultaneously with zero heap allocations in critical tick loops (~14 MB RAM footprint, sub-millisecond execution).
* **3-Regime Orthogonal Strategy Basket:**
  1. **Mode 1 (Liquidity Wick Hunter - Agent-040):** S/R level wick rejection and absorption.
  2. **Mode 2 (Donchian Breakout - Agent-433):** Channel breakouts with volatility expansion (`vol_bonus`).
  3. **Mode 3 (EMA21 Pullback Hunter):** Trend retest and continuation entries.
* **Dual-Brain Artificial Intelligence Arbitrage:**
  - **TypeSafe JEV (System-1 Decision Filter):** Pre-evaluates entries against 14 institutional order-flow factors (Taker CVD, Whale Top-Long Ratio, Open Interest Delta, BTC momentum) with hard-veto threshold (<85 score rejected).
* **Pine Script REA Integration:** Native translation of Pine Script pivot support/resistance levels directly mapped to real-time tick charts.
* **Institutional Capital Protection:**
  - **Zero-Liquidation Armor:** 5x Isolated Margin with hard exchange-level `STOP_MARKET` orders.
  - **🚨 BTC Emergency Crash Brake:** Microsecond liquidation protection detecting sudden BTC drops (>-0.35%) to immediately secure altcoin runner profits and freeze new risks.
  - **2.0x ATR Breakeven Lock:** Relaxes early exit choking, giving trades room to breathe until reaching genuine trend velocity (+2.0x ATR).
  - **2.0x ATR Trailing Stop:** Captures 1:2.5 to 1:3 Risk/Reward macro moves.
  - **Dynamic Compound Growth:** Auto-scales risk with equity (`Risk = (Balance * 2%)`).

---

## 📊 Live Web Dashboard & Telemetry Terminal

* **Port 8080 Control Terminal:**
  - Embedded **TradingView Lightweight Charts** with real-time candlestick streams, dynamic Pine Script support/resistance channels, and order book depth lines.
  - **Order Flow Ribbon:** Live monitoring of Buy/Sell walls, Taker Buy %, Whale Long %, Open Interest Delta %, and Order Book Imbalance ratio.
  - **13-Pair Multi-Radar Grid:** Real-time distance to SMA50, RSI(14), trend status, and exact top-20 order book depth accumulation (`🛡️ Alım Duvarı | 🚧 Satış Duvarı`).
  - **One-Click Trading Control:** Toggle autonomous trading active/paused (`⚡ OTONOM İŞLEM: AKTİF` / `🛑 OTONOM İŞLEM: DURDURULDU`) directly from the browser header.

---

## 🚀 Tracked Symbols (13 Majors)

| Symbol | Leverage | Min Volatility Filter | Typical Regime |
| :--- | :---: | :---: | :---: |
| **BTCUSDT** | 5x | ATR / Price >= 0.12% | Market Anchor / Momentum Leader |
| **ETHUSDT** | 5x | ATR / Price >= 0.15% | Trend Breakout |
| **SOLUSDT** | 5x | ATR / Price >= 0.20% | Pullback & Liquidity Hunter |
| **XRPUSDT** | 5x | ATR / Price >= 0.25% | S/R Breakout & Absorption |
| **DOGEUSDT** | 5x | ATR / Price >= 0.30% | Volatility Expansion |
| **NEARUSDT** | 5x | ATR / Price >= 0.35% | Breakout & Trend Runner |
| **LINKUSDT** | 5x | ATR / Price >= 0.25% | EMA21 Retest |
| **AVAXUSDT** | 5x | ATR / Price >= 0.30% | Donchian Channel Breakout |
| **SUIUSDT** | 5x | ATR / Price >= 0.35% | Momentum Continuation |
| **ADAUSDT** | 5x | ATR / Price >= 0.30% | Range Wick Hunter |
| **APTUSDT** | 5x | ATR / Price >= 0.35% | Liquidity Wick Absorption |
| **DOTUSDT** | 5x | ATR / Price >= 0.30% | Pullback Support |
| **TIAUSDT** | 5x | ATR / Price >= 0.40% | High-Beta Trend Surfer |

---

## 🛠️ Quick Start & Deployment

### 1. Compile & Build
```bash
cargo build --release
```

### 2. Configure Credentials
Place your Binance Futures API credentials in `testnet_keys.json` (or `mainnet_keys.json` on production servers):
```json
{
  "api_key": "YOUR_BINANCE_API_KEY",
  "api_secret": "YOUR_BINANCE_API_SECRET"
}
```

### 3. Run Web Server & Bot
```bash
# Start Web Server & API (Port 8080)
python3 web_server.py &

# Start Order Book Depth Tracker
python3 depth_tracker.py &

# Launch Rust Quant Bot
./target/release/binance_futures_bot
```

### 4. Systemd Service Deployment (Production EC2)
Services run autonomously under systemd:
- `binance-futures-bot.service` (Main Rust Engine)
- `bot-dashboard.service` (Python Web Server & Control API)
- `binance-depth-tracker.service` (Order Book Depth Accumulation Tracker)

---

## 🔒 Security & Privacy Notice
* Live exchange secrets and `.pem` SSH keys are strictly ignored via `.gitignore`.
* System state is persisted atomically via `bot_state.json` and synchronized with Binance exchange positions every 30 seconds.
