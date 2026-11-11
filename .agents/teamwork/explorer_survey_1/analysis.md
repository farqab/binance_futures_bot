# Comprehensive Backtest Records, Metrics, and Evaluation Survey

**Repository:** `E:\binance_futures_bot`  
**Explorer:** `explorer_survey_1`  
**Date:** 2026-11-06  
**Status:** Complete  

---

## 1. Executive Summary

A comprehensive, read-only survey of `E:\binance_futures_bot` was conducted to map all existing backtest records, historical logs, data feeds, performance metrics, data structures, and evaluation scripts. 

Key high-level findings include:
1. **No Persistent Local Backtest Datasets**: There are **zero** static CSV, SQLite, Parquet, or JSON historical candle files stored in the repository. All backtesting relies on **live, on-the-fly network calls** to the Binance Futures REST API (`https://fapi.binance.com/fapi/v1/klines`) using `curl.exe` in Rust or `urllib.request` in Python.
2. **Ephemeral Output via STDOUT**: Backtest results are not written to structured output files (`.json`, `.csv`, or SQLite databases); they are formatted and printed exclusively to standard terminal output (`stdout`).
3. **Monolithic Rust Engine with Embedded Backtest**: The primary backtest implementation is contained inside `src/main.rs` (lines 1490–2061: `run_native_rust_backtest`), invoked via `cargo run -- --backtest`.
4. **Binary vs. Source Code Version Drift**:
   - The pre-compiled binary `target/debug/binance_futures_bot.exe` dates from **2026-11-04 23:02:13** and runs an older **v5.0 Dual-Engine** backtest over 6 coins on 15-minute candles.
   - The current source code `src/main.rs` was updated on **2026-11-06 18:35:10** to a **v7.5/v8.3 Tri-Regime Engine** over 13 coins on 1-hour candles with a walk-forward calendar model.
5. **Critical Quantitative Metric Gaps**: The existing backtest engine tracks total return, win rate, profit factor, max drawdown, and fees, but **completely lacks time-in-trade duration tracking, time-weighted ROI (ROI / unit time), Sharpe ratio, Sortino ratio, Calmar ratio, trade expectancy, and per-coin performance attribution**.

---

## 2. Complete Filesystem & Artifact Inventory

The table below catalogs every file identified in `E:\binance_futures_bot`:

| Path | Size | Type | Role in Backtesting / Strategy Evaluation |
| :--- | :--- | :--- | :--- |
| `src/main.rs` | 136,987 B | Rust Source (3,454 lines) | Contains core execution engine, live WebSocket feeds, technical indicators, TypeSafe JEV arbiter, and `run_native_rust_backtest` (lines 1490–2061). |
| `BOT_ARCHITECTURE_SPEC.md` | 9,244 B | Documentation | Markdown spec documenting system architecture, mathematical risk model, and 30-day comparative benchmark table (lines 99–106). |
| `bot_state.json` | 354 B | JSON State File | Atomic persistence file storing current live balance ($9.88), peak equity, max drawdown, trade counters, and `closed_trades` array. |
| `telemetry.js` | 2,013 B | JavaScript State Bridge | Generates `window.__RUST_STATE__` object for browser dashboard; stores real-time indicator snapshots and trade counts. |
| `dashboard.html` / `index.html` | 29,923 B | Web UI Dashboard | Real-time monitoring frontend (order flow, Donchian sweep, live ΔOI, active position). |
| `Cargo.toml` | 292 B | Build Manifest | Rust dependencies: `tungstenite` 0.23, `serde` 1.0, `serde_json` 1.0, `hmac` 0.12, `sha2` 0.10, `chrono` 0.4, `anyhow` 1.0. |
| `Cargo.lock` | 22,515 B | Lockfile | Deterministic dependency tree. |
| `deploy_aws.sh` | 2,952 B | Shell Script | AWS EC2 deployment automation. References `./target/release/binance_futures_bot --backtest` at line 81. |
| `project.tar.gz` | 15,869 B | Archive | Deployment tarball containing `Cargo.toml`, `src/main.rs`, and `deploy_aws.sh`. |
| `testnet_keys.json` | 104 B | JSON Credentials | Template for Binance Futures testnet API keys. |
| `scratch/test_params.py` | 1,336 B | Python Script | Standalone kline fetcher for 13 symbols using `urllib.request` against Binance Futures REST API. |
| `scratch/read_article.py` | 686 B | Python Script | Utility parser for Twitter/TreeNews article payloads. |
| `scratch/test_ws.py` | 811 B | Python Script | Diagnostic client for `wss://news.treeofalpha.com/ws`. |
| `target/debug/binance_futures_bot.exe` | 23,022,873 B | Binary (Windows PE) | Compiled debug executable from 2026-11-04 (v5.0 engine). |
| `.agents/teamwork/` | - | Directory | Orchestrator and explorer teamwork coordination workspace. |

---

## 3. The Backtest Engine Architecture (`src/main.rs`)

### 3.1 Entry Point & Invocation
In `src/main.rs`, command-line arguments are parsed in `fn main()`:
```rust
// Lines 2921-2923
if args.iter().any(|a| a == "--backtest") {
    return run_native_rust_backtest(&args);
}
```

The function `run_native_rust_backtest(args: &[String]) -> Result<()>` supports the following CLI options:
- `--days <N>`: Custom backtest window in days (default: 30 days).
- Quick flags: `--month` / `--30d` (30 days), `--90d` / `--3m` (90 days), `--20d` (20 days), `--10d` (10 days), `--all` (90 days).
- Timeframe selection: `--5m`, `--15m`, or default `"1h"`.

### 3.2 Data Ingestion Pipeline
Unlike traditional quant platforms that query local databases, this engine downloads klines directly from Binance Futures public REST endpoints via `curl.exe`:
- **Endpoint:** `https://fapi.binance.com/fapi/v1/klines?symbol={symbol}&interval={tf}&limit=1500`
- **Chunking Mechanism:** For test windows exceeding 50 days, it performs a two-stage chunked request using `&endTime={first_ts - 1}` to fetch up to 3,000 historical bars per coin.
- **Coverage:** Tested across 13 symbols: `ETHUSDT, XRPUSDT, SOLUSDT, DOGEUSDT, LINKUSDT, BTCUSDT, SUIUSDT, AVAXUSDT, NEARUSDT, ADAUSDT, APTUSDT, DOTUSDT, TIAUSDT`.
- **Parsing:** Raw JSON responses are parsed into standard 6-tuples: `(ts, open, high, low, close, volume)`.

### 3.3 Simulation Loop & Execution Mechanics
1. **Warmup Phase:** Indicators (`SymbolEngine`) are seeded for each coin over historical candles before the evaluation window starts (`start_test_idx = min_len.saturating_sub(bars_to_test).max(100)`).
2. **Serial Position Constraint (`active: Option<...>`):** The engine enforces a **single concurrent position** across the entire 13-coin universe. If one coin holds an open position, all other coins are locked out until it closes.
3. **Execution Modeling:**
   - **Entry Execution:** Orders fill on the opening price of the subsequent bar plus slippage: `next_open * (1.0 +/- SLIPPAGE_RATE)` (SLIPPAGE_RATE = 0.0001 / 1 bps).
   - **Fee Model:** Taker fee applied on both entry and exit: `exec_price * qty * TAKER_FEE_RATE` (TAKER_FEE_RATE = 0.0005 / 5 bps).
   - **Stop-Loss / Take-Profit Sizing:**
     - SL: `1.8x ATR` (line 37: `SL_ATR_MULT = 1.8`).
     - TP: `3.6x ATR` (line 38: `TP_ATR_MULT = 3.6`, giving an initial 1:2.0 asymmetric Risk/Reward ratio).
   - **Dynamic Profit Protection:**
     - At `+1.5x ATR` excursion: Stop-loss is moved to `entry * 1.0025` (+0.25% net profit lock above fees).
     - At `+2.2x ATR` excursion: Trailing stop locks `peak_price - 1.2x ATR`.
   - **Emergency Exit Filter:** `check_emergency_exit_typesafe_jev` checks adverse news or severe BTC counter-trend signals to trigger immediate market exit.
   - **Daily Circuit Breakers:**
     - Daily target lock: If daily net profit reaches `max(0.42, balance * 0.045)`, trading halts for the day.
     - Daily loss limit: If daily loss reaches `-max(0.65, balance * 0.08)` or if 2 losing trades occur in a day (`d.3 >= 2`), trading is blocked until the next day.
     - Cooldown: Losing trades impose a 10-bar cooldown on the specific coin (`cooldown_until.insert(sym, i + 10)`).

---

## 4. Performance Metrics: Catalog & Gap Analysis

### 4.1 Metrics Currently Calculated

| Category | Metric Name | Definition / Formula | Location in Code |
| :--- | :--- | :--- | :--- |
| **Trade** | Net PnL ($) | `gross_pnl - entry_fee - exit_fee` | `src/main.rs:1776` |
| **Trade** | Gross PnL ($) | `(exit - entry) * qty` (Long) / `(entry - exit) * qty` (Short) | `src/main.rs:1771-1775` |
| **Trade** | Fees Paid ($) | `entry_fee + exit_fee` (5 bps per side) | `src/main.rs:1770, 1778` |
| **Trade** | Slippage ($) | Executed at `price * (1 +/- 0.0001)` | `src/main.rs:1765-1769` |
| **Daily** | Daily Start / End Bal | Beginning and ending equity per UTC calendar day | `src/main.rs:1657, 1781` |
| **Daily** | Daily Win / Loss Count | Trades closed in profit vs loss during day | `src/main.rs:1784, 1788` |
| **Daily** | Daily Net PnL & % | Total net dollars and percentage return for day | `src/main.rs:2003-2006` |
| **Portfolio**| Final Balance | Cumulative equity starting from $10.00 | `src/main.rs:2042` |
| **Portfolio**| Net Return % | `((Final Balance - Initial Balance) / Initial Balance) * 100` | `src/main.rs:2040` |
| **Portfolio**| Win Rate % | `(Winning Trades / Total Trades) * 100` | `src/main.rs:2030-2034` |
| **Portfolio**| Profit Factor | `Gross Winning PnL / Gross Losing PnL` | `src/main.rs:2035-2039` |
| **Portfolio**| Total Fees Paid | Sum of all taker fees | `src/main.rs:2057` |
| **Portfolio**| Max Drawdown % | `max(((Peak Equity - Current Equity) / Peak Equity) * 100)` | `src/main.rs:1803-1806` |
| **Portfolio**| Avg Daily Return ($ & %) | Net return divided by number of calendar days | `src/main.rs:2041-2043` |

### 4.2 Critical Missing Metrics (Identified Gaps)

The table below highlights essential quant metrics completely absent from the current backtest output:

| Missing Metric | Importance for Time-Based Optimization | Difficulty to Implement |
| :--- | :--- | :--- |
| **Time in Trade (Holding Period / Bars in Trade)** | **Critical**. Necessary to evaluate capital velocity and calculate profit per unit time. Currently, entry timestamp or bar index is not stored in `active`. | Low (add `entry_bar_idx` / `entry_ts` to `active` tuple). |
| **Time-Weighted ROI (ROI / Hour or ROI / Day)** | **Critical (Core requirement R2)**. Measures capital efficiency so strategies that produce +1% in 2 hours are prioritized over +1.5% in 3 days. | Low once holding period is tracked. |
| **Sharpe Ratio (Daily & Annualized)** | **High**. Standard measure of risk-adjusted return ($(\mu - r_f) / \sigma$). | Low (derive from vector of daily returns). |
| **Sortino Ratio** | **High**. Downside-deviation adjusted return; penalizes harmful volatility without penalizing upside surges. | Medium. |
| **Calmar Ratio** | **High**. Annualized return divided by Maximum Drawdown ($\text{CAGR} / \text{MaxDD}$). | Low. |
| **Trade Expectancy ($ & R-multiple)** | **Medium**. Average expected gain per trade: $(\text{WR} \times \text{AvgWin}) - ((1 - \text{WR}) \times \text{AvgLoss})$. | Low. |
| **Per-Coin Performance Attribution** | **High**. Shows which of the 13 coins generated profits vs drag. Currently all coins are pooled. | Low (track win/loss/PnL map by symbol). |
| **Long vs. Short Attribution** | **Medium**. Determines whether bullish or bearish market regimes drive performance. | Low. |
| **Maximum Adverse Excursion (MAE)** | **Medium**. Deepest unrealized drawdown experienced while in a trade before exit. | Medium. |
| **Maximum Favorable Excursion (MFE)** | **Medium**. Highest unrealized profit reached before exit (checks if TP is leaving too much on the table). | Medium (engine tracks `peak_p`, but discards it upon exit). |

---

## 5. Data Structures & Schema Documentation

### 5.1 Kline Ingestion Schema (Binance REST API)
- Source: `GET https://fapi.binance.com/fapi/v1/klines`
- Raw element indices:
  - `0`: Open time (Unix milliseconds `i64`)
  - `1`: Open price (`f64`)
  - `2`: High price (`f64`)
  - `3`: Low price (`f64`)
  - `4`: Close price (`f64`)
  - `5`: Volume (`f64`)
- In-memory tuple representation in `raw_candles: HashMap<String, Vec<...>>`:
  ```rust
  (i64, f64, f64, f64, f64, f64) // (timestamp_ms, open, high, low, close, volume)
  ```

### 5.2 Active Trade State Schema (`active` in `src/main.rs:1617`)
When a trade is active, it is stored as an `Option<Tuple>`:
```rust
Option<(
    String,        // 0: Symbol (e.g. "SOLUSDT")
    &'static str,  // 1: Side ("LONG" or "SHORT")
    f64,           // 2: Executed entry price
    f64,           // 3: Executed quantity (coin units)
    f64,           // 4: Current Stop-Loss price (mutable via trailing lock)
    f64,           // 5: Take-Profit price
    f64,           // 6: Peak price observed during trade (for trailing stop)
    f64,           // 7: Trailing active flag (bool)
    f64,           // 8: Entry fee paid in USD
    f64,           // 9: ATR-14 at entry
)>
```
*Gap*: Neither entry bar index nor entry timestamp is stored here, making trade duration impossible to reconstruct without modification.

### 5.3 Daily Stats Map Schema (`daily_stats` in `src/main.rs:1623`)
```rust
HashMap<
    String, // Date key: "YYYY-MM-DD"
    (
        f64,         // 0: Day opening balance
        f64,         // 1: Day closing balance
        u32,         // 2: Day winning trade count
        u32,         // 3: Day losing trade count
        f64,         // 4: Day cumulative net PnL ($)
        Vec<String>, // 5: Closed trade detail strings (e.g. "SOL LONG (+0.32$)")
    )
>
```

### 5.4 Production Closed Trade Schema (`ClosedTradeRecord` in `src/main.rs:253`)
Used in live trading and saved into `bot_state.json`:
```rust
struct ClosedTradeRecord {
    pub timestamp: String,    // ISO-8601 string
    pub symbol: String,       // e.g. "BTCUSDT"
    pub side: String,         // "LONG" | "SHORT"
    pub leverage: u32,        // e.g. 6 or 10
    pub qty: f64,             // Coin quantity
    pub margin_used: f64,     // USDT margin
    pub entry_price: f64,     // Entry price
    pub exit_price: f64,      // Exit price
    pub gross_pnl: f64,       // Gross PnL in USD
    pub fees_paid: f64,       // Entry + Exit fees in USD
    pub net_pnl: f64,         // Net realized PnL in USD
    pub roe_pct: f64,         // Return on Equity %
    pub reason: String,       // Exit trigger: "TP", "SL", "EmergencyExit"
}
```

### 5.5 Backtest Terminal Output Format
The backtest prints two tables:
1. **Daily Calendar Breakdown Table**:
   ```
   Tarih        | Gün Başı -> Sonu   | İşlem(K/Z) | Günlük Net K/Z | Kapanan İşlem Detayları
   2026-11-04   | $13.50  -> $16.97  |  5 (3K/2Z) |  +3.43$ (+25.7%) | SOL LONG (-0.66$), ETH LONG (+2.54$)...
   ```
2. **Cumulative Summary Banner**:
   ```
   🏆 [GENEL TOPLAM (N Takvim Günü | Mum Sayısı)]
      Başlangıç: $10.00 -> Son Kasa: $X.XX (+Y.YY%) | Ortalama Günlük Getiri: +$Z.ZZ (+W.WW% / gün)
      Toplam İşlem: T (W Kazanan / L Kaybeden -> WinRate: %WR) | Profit Factor: PF | Toplam Fee: -$Fee | Max DD: %DD
   ```

---

## 6. Discrepancies, Gaps, and Technical Obstacles

### 6.1 Discrepancy: Binary vs. Source Code Version
- **Observed**: Running `target/debug/binance_futures_bot.exe --backtest --10d` outputs:
  ```
  📅 NATIVE RUST 15m GÜNLÜK TAKVİM BACKTEST & DOĞRULAMA MOTORU (v5.0 Dual-Engine)
  6 Coin Birleşik (1499x6=8994 Mum)
  ```
- **Code Reality**: In `src/main.rs`, the backtest is defined as:
  ```rust
  println!("📅 NATIVE RUST 1h GÜNLÜK TAKVİM BACKTEST MOTORU (Institutional Liquidity Sweep & Wick Exhaustion v7.1)");
  println!("Kapsam: {} Gün ({} Saat) (13 Parite)...");
  ```
- **Cause**: The executable was compiled on **November 4, 2026**, prior to the November 6 updates that added 7 new altcoins (SUI, AVAX, NEAR, ADA, APT, DOT, TIA) and the v7.5 / v8.3 tri-regime engine.

### 6.2 Toolchain Obstacle: MinGW Linker Failure
- When attempting to build or run the updated Rust code (`cargo run -- --backtest`), the linker fails:
  ```
  error: linking with `x86_64-w64-mingw32-gcc` failed: exit code: 1
  ld: cannot find crt2.o: No such file or directory
  ld: cannot find -lkernel32: No such file or directory
  ```
- The Rust compiler is configured with `stable-x86_64-pc-windows-gnu` but the MinGW runtime library paths are not present in the default library search directory.
- `cargo check` succeeds with zero errors (only 6 minor warnings).
- **Practical Implication**: Fast backtesting, parameter sweeps, and time-based metrics calculations can be executed directly and reliably using Python (`scratch/test_params.py` verified to fetch 1,499 candles across 13 symbols in 6.0 seconds), while Rust code can be verified via `cargo check` and deployed to Linux/AWS via `deploy_aws.sh`.

### 6.3 Historical Data Volatility & Non-Determinism
- Because klines are fetched live from Binance without local caching:
  - Backtests run on different days evaluate shifting historical windows (the last 1,500 bars slide forward with every hour).
  - An offline environment or Binance API rate-limit immediately halts execution.
  - Historical order flow metrics (Taker Buy %, Delta Open Interest %) are not stored in historical klines by Binance and must be approximated or reconstructed during backtesting.

### 6.4 Single-Position Bottleneck
- In `run_native_rust_backtest`:
  - `active: Option<(String, ...)>` restricts the bot to **1 single trade at a time** across all 13 symbols.
  - If a low-momentum coin triggers first, a high-conviction breakout on ETH or SOL is skipped.
  - This severely constrains time-based ROI because capital is tied up in a single position while other high-performing opportunities expire.

---

## 7. Concrete Next Steps for Downstream Workstreams

1. **For Explorer 2 (Strategies & Execution Engine)**:
   - Map the 3 regimes in `recompute_confluence`:
     - Regime 1: Donchian Liquidity Sweep (Wick exhaustion)
     - Regime 2: Momentum Breakout (Donchian high/low break + volume)
     - Regime 3: Dynamic EMA21 Pullback
   - Inspect the TypeSafe JEV Arbiter (`typesafe_unified_arbiter`) and BTC macro anchor filters.
2. **For Explorer 3 (Time-Based Metrics & Optimization)**:
   - Design the Time-in-Trade calculation engine:
     $$\text{ROI}_{\text{time}} = \frac{\text{Net Return \%}}{\text{Holding Hours}}$$
   - Formulate Sharpe Ratio, Sortino Ratio, and Calmar Ratio formulas for the discrete trade logs.
   - Design multi-position allocation or fast-exit rules (time-stop after $N$ bars without expansion).
3. **For Milestone 1 (Data Ingestion & Benchmark Standardization)**:
   - Establish a standardized, cached dataset of 1h and 15m candles for the 13 symbols to ensure reproducible, zero-latency evaluations across all optimization runs.
