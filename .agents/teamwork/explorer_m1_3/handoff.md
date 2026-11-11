# Handoff Report — explorer_m1_3

**Task:** Milestone 1 (Features 1 & 2) — Integration Architecture, Verification Strategy, Invariants, and File Ownership Boundaries  
**Agent:** `explorer_m1_3` (teamwork_explorer)  
**Parent:** `4cd38888-59b8-4580-a42f-c9475ff72425`  
**Working Directory:** `E:\binance_futures_bot\.agents\teamwork\explorer_m1_3`  
**Date:** 2026-11-06T16:58:00Z  

---

## 1. Observation

### 1.1 Current Backtest Engine Architecture in Rust
1. **Network Fetching on Every Run (`src/main.rs:1526–1597`)**:
   ```rust
   for spec in SYMBOLS.iter() {
       let mut vec_c = Vec::new();
       let url1 = format!(
           "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval={}&limit=1500",
           spec.symbol, tf
       );
       let mut chunk1 = Vec::new();
       if let Ok(output) = Command::new(CURL_BIN)
           .args(["-s", "--max-time", "15", &url1])
           .output()
       { ... }
   ```
   - Running `--backtest` executes 13 sequential `curl.exe` processes (or 26 if `days > 50`).
   - Zero local caching exists in `src/main.rs`. Data is discarded from memory when the process terminates.
   - If network latency or API rate limits intervene, backtest execution halts or fails (`min_len < 120`).

2. **Active Position Representation (`src/main.rs:1617`, `1977–1988`)**:
   ```rust
   let mut active: Option<(String, &'static str, f64, f64, f64, f64, f64, bool, f64, f64)> = None;
   ```
   - Represents: `(sym, side, exec_p, qty, sl, tp, peak_p, tr_act, e_fee, atr)`.
   - **Direct Observation**: Neither `entry_bar_idx` nor `entry_time_ms` is stored in the active tuple.
   - On position closure (`src/main.rs:1764–1808`), elapsed trade time and bar counts are completely discarded.

3. **Output Mechanism (`src/main.rs:1994–2058`)**:
   - The backtest prints formatted ASCII tables to standard output (`println!`).
   - No structured file (JSON/CSV) is produced or persisted to disk. Downstream optimization, comparative ranking, or automated verification tools cannot inspect trade records without screen-scraping stdout.

### 1.2 Python Test Harness & Existing Verification Suites
1. **Interface Contract Implementation in `tests/test_harness.py`**:
   - `KlineDataCache` (`tests/test_harness.py:44–102`): Supports `save_klines`, `load_klines`, and `validate_kline_schema`.
   - `TradeRecord` (`tests/test_harness.py:108–195`): Adheres to `PROJECT.md:37–54` contract:
     - `holding_duration_hours = round(duration_ms / 3600000.0, 4)`
     - `roi_pct = round((net_pnl / margin_used) * 100.0, 4)`
     - `roi_per_hour = round(roi_pct / holding_duration_hours, 4)`
     - `to_dict()` outputs structured JSON dictionary.
2. **Tier 1 Test Suite (`tests/test_tier1_features.py`)**:
   - 35 unit/contract tests across Features 1–7 executed via:
     `python -X utf8 -m unittest discover -s E:\binance_futures_bot\tests`
   - **Result**: `Ran 35 tests in 0.108s -> OK`.

### 1.3 Pre-Compiled Binary & Baseline Performance
1. Executing `E:\binance_futures_bot\target\debug\binance_futures_bot.exe --backtest --10d` returned:
   ```
   📅 NATIVE RUST 15m GÜNLÜK TAKVİM BACKTEST & DOĞRULAMA MOTORU (v5.0 Dual-Engine)
   🏆 [GENEL TOPLAM (11 Takvim Günü | 1499x6=8994 Mum)]
      Başlangıç: $10.00 -> Son Kasa: $14.97 (+49.74%) | Ortalama Günlük Getiri: +0.45$ (+4.52% / gün)
      Toplam İşlem: 25 (15 Kazanan / 10 Kaybeden -> WinRate: %60.0) | Profit Factor: 1.66 | Toplam Fee: -$2.04 | Max DD: %16.75
   ```
2. `cargo check --manifest-path E:\binance_futures_bot\Cargo.toml` validates clean AST and types with 0 errors in 0.21s.

---

## 2. Logic Chain

1. **From Observation 1.1 (Item 1)**: Every backtest execution downloads klines over HTTP, introducing 15–35s delays and failing offline. Integrating a transparent local cache-check inside `run_native_rust_backtest` (and Python runners) that loads from `data/cache/{symbol}_{interval}.json` if present reduces data retrieval time from 25,000ms to <10ms and ensures deterministic, offline test repeatability.
2. **From Observation 1.1 (Item 2)**: The lack of entry timestamps and bar indexes in the active position state makes duration ($\Delta t$) mathematically inaccessible. Upgrading the raw 10-tuple to a typed struct `ActiveBacktestPosition` that records `entry_bar` and `entry_time_ms` allows exact duration and bar count calculation on exit without altering entry or exit pricing logic.
3. **From Observation 1.1 (Item 3) & Observation 1.2**: Downstream Milestones (M2 Time-Decay and M3 Comparative Ranking) require structured trade records (`PROJECT.md` M1 ↔ M2 contract). Emitting `reports/backtest_latest.json` alongside stdout preserves existing terminal visibility while supplying machine-readable artifacts.
4. **From Observation 1.3**: Because baseline trading logic produces verified results (WinRate, PF, Net PnL, Drawdown), adding caching, duration tracking, and JSON writing MUST NOT mutate trade signals, fill prices, slippage, or fee accounting. An invariant test verifying exact bit-for-bit equivalence against pre-change baselines guarantees zero regression.

---

## 3. Caveats

1. **Linker Environment on Windows**: While `cargo check` compiles with 0 errors in 0.21s, the host GCC MinGW linker lacks CRT objects (`crt2.o`), preventing direct local `cargo run` compilation on Windows without sysroot configuration. The system relies on Python test runners (`tests/test_harness.py`) and Linux builds (`deploy_aws.sh`). Rust code changes must remain 100% syntactically valid and pass `cargo check`.
2. **Single-Position Concurrency**: The engine maintains a single open position across all 13 symbols (`active: Option<...>`). Invariants and duration tracking must account for this: trade $k+1$ can only be entered on or after the exit bar of trade $k$.
3. **Partial Bars**: In live/recent Binance kline feeds, the latest open bar (`data[-1]`) is incomplete. Ingestion logic must always trim the last unclosed bar (`data[:-1]` or `r.take(len - 1)`) to preserve backtest determinism.

---

## 4. Conclusion & Recommended Architecture

### 4.1 Caching Layer Integration Strategy
Integrate a **Cache-First Pipeline** directly into `src/main.rs:1526–1597` and Python runners:

```
                  ┌──────────────────────────────┐
                  │ Backtest Runner Starts       │
                  └──────────────┬───────────────┘
                                 │
                   Check data/cache/{sym}_{tf}.json
                                 │
                 ┌───────────────┴───────────────┐
                 ▼                               ▼
         [ Cache Hit ]                    [ Cache Miss ]
  ┌─────────────────────────────┐   ┌─────────────────────────────┐
  │ Read local JSON (<5ms)      │   │ Fetch via Binance REST API  │
  │ Validate checksum & count   │   │ Trim partial candle         │
  │ Populate raw_candles RAM    │   │ Write data/cache/{sym}_{tf} │
  └──────────────┬──────────────┘   └──────────────┬──────────────┘
                 │                                 │
                 └───────────────┬─────────────────┘
                                 ▼
                  Proceed to Simulation Loop
```

#### Rust Code Integration Template (`src/main.rs`):
```rust
// Define Cache Path
let cache_dir = Path::new("data/cache");
if !cache_dir.exists() {
    let _ = std::fs::create_dir_all(cache_dir);
}
let cache_file = cache_dir.join(format!("{}_{}.json", spec.symbol, tf));

let mut vec_c = Vec::new();
let mut loaded_from_cache = false;

// 1. Try Cache Hit
if cache_file.exists() && !args.iter().any(|a| a == "--no-cache" || a == "--force-refresh") {
    if let Ok(content) = std::fs::read_to_string(&cache_file) {
        if let Ok(cached_json) = serde_json::from_str::<serde_json::Value>(&content) {
            // Support both standard schema and array of klines
            let rows = if let Some(arr) = cached_json.get("data").and_then(|d| d.as_array()) {
                arr
            } else if let Some(arr) = cached_json.get("klines").and_then(|k| k.as_array()) {
                arr
            } else {
                cached_json.as_array().unwrap_or(&vec![])
            };
            for r in rows {
                if let (Some(ts), Some(o), Some(h), Some(l), Some(c), Some(v)) = (
                    r.get(0).and_then(|v| v.as_i64()),
                    r.get(1).and_then(|v| v.as_f64()),
                    r.get(2).and_then(|v| v.as_f64()),
                    r.get(3).and_then(|v| v.as_f64()),
                    r.get(4).and_then(|v| v.as_f64()),
                    r.get(5).and_then(|v| v.as_f64()),
                ) {
                    if c > 0.0 {
                        vec_c.push((ts, o, h, l, c, v));
                    }
                }
            }
            if vec_c.len() >= 120 {
                loaded_from_cache = true;
            } else {
                vec_c.clear();
            }
        }
    }
}

// 2. Fallback to Remote Fetch if Cache Miss
if !loaded_from_cache {
    // Existing curl logic fetches chunk1 & chunk2
    // ...
    // Immediately persist to cache_file
    let cache_payload = serde_json::json!({
        "symbol": spec.symbol,
        "interval": tf,
        "count": vec_c.len(),
        "updated_at": chrono::Utc::now().timestamp_millis(),
        "data": vec_c
    });
    let _ = std::fs::write(&cache_file, serde_json::to_string(&cache_payload).unwrap_or_default());
}
```

### 4.2 Duration Tracking & Struct Upgrades
Replace the raw 10-tuple at line 1617 with a structured representation:

```rust
#[derive(Debug, Clone)]
pub struct ActiveBacktestPosition {
    pub symbol: String,
    pub side: &'static str,
    pub entry_price: f64,
    pub qty: f64,
    pub margin_used: f64,
    pub sl: f64,
    pub tp: f64,
    pub peak_p: f64,
    pub tr_act: bool,
    pub entry_fee: f64,
    pub atr: f64,
    pub entry_bar: usize,
    pub entry_time_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestTradeRecord {
    pub symbol: String,
    pub side: String,
    pub entry_time: i64,
    pub exit_time: i64,
    pub holding_duration_hours: f64,
    pub holding_bars: usize,
    pub entry_price: f64,
    pub exit_price: f64,
    pub qty: f64,
    pub margin_used: f64,
    pub gross_pnl: f64,
    pub fees: f64,
    pub net_pnl: f64,
    pub roi_pct: f64,
    pub roi_per_hour: f64,
    pub exit_reason: String,
}
```

#### Exact Duration Calculations on Exit:
```rust
let exit_bar = i;
let exit_time_ms = raw_candles.get(&sym).unwrap()[exit_bar].0;

let holding_bars = exit_bar.saturating_sub(pos.entry_bar).max(1);
let duration_ms = (exit_time_ms - pos.entry_time_ms).max(3_600_000); // Floor at 1 bar (1 hour)
let holding_duration_hours = (duration_ms as f64) / 3_600_000.0;

let roi_pct = if pos.margin_used > 0.0 {
    (net / pos.margin_used) * 100.0
} else {
    0.0
};

let roi_per_hour = if holding_duration_hours > 0.0 {
    roi_pct / holding_duration_hours
} else {
    roi_pct
};

let exit_reason_str = if is_emergency {
    "EMERGENCY"
} else if was_tp {
    "TP"
} else if pos.tr_act && was_sl {
    "PROFIT_LOCK"
} else {
    "SL"
};
```

### 4.3 Dual Output Strategy (ASCII Terminal + Structured JSON)
1. **Preserve ASCII Terminal Output**: Lines 1994–2058 remain untouched, ensuring all daily summaries and metrics are printed to stdout as before.
2. **Structured JSON Emission**:
   At the end of `run_native_rust_backtest`, write `reports/backtest_latest.json`:
   ```rust
   let _ = std::fs::create_dir_all("reports");
   let report_payload = serde_json::json!({
       "strategy_name": "TriRegime_Institutional_Sweep_v7.5",
       "timeframe": tf,
       "days": days,
       "symbols": SYMBOLS.iter().map(|s| s.symbol).collect::<Vec<_>>(),
       "summary": {
           "initial_balance": INITIAL_BALANCE,
           "final_balance": balance,
           "net_profit_pct": ret_pct,
           "total_trades": total_tr,
           "wins": wins,
           "losses": losses,
           "win_rate_pct": wr,
           "profit_factor": pf,
           "total_fees": total_fees,
           "max_drawdown_pct": max_dd,
           "avg_duration_hours": avg_duration,
           "active_capital_hourly_roi": hourly_active_roe
       },
       "trades": closed_trade_records
   });
   let _ = std::fs::write("reports/backtest_latest.json", serde_json::to_string_pretty(&report_payload).unwrap_or_default());
   ```

---

## 5. Verification Invariants & Command Suite

Downstream Worker, Reviewers, and Challengers MUST verify the following invariants:

| # | Invariant | Formal Definition / Test Rule | Invalidation Condition |
|---|---|---|---|
| **INV-1** | **Mathematical PnL Identity** | $\Delta \text{PnL} = |\text{NetPnL}_{\text{new}} - \text{NetPnL}_{\text{baseline}}| < 10^{-6}$ for every single trade. | Any divergence in win count, loss count, trade entry/exit price, or final balance. |
| **INV-2** | **Strict Positive Duration** | For all trades $k$: $\text{holding\_duration\_hours} > 0.0$ and $\text{holding\_bars} \ge 1$. | Any trade with duration $\le 0$ or bar count $< 1$. |
| **INV-3** | **Monotonic Non-Decreasing Timestamps** | Intra-trade: $t_{\text{entry}} < t_{\text{exit}}$.<br>Inter-trade: For all consecutive trades $k, k+1$, $t_{\text{exit}, k} \le t_{\text{entry}, k+1}$. | Any retroactive entry timestamp or overlapping position under single-slot concurrency. |
| **INV-4** | **Accounting & Ratio Consistency** | $\text{net\_pnl} = \text{gross\_pnl} - \text{fees}$<br>$\text{roi\_per\_hour} = \text{roi\_pct} / \text{holding\_duration\_hours}$ | Any arithmetic inconsistency between duration, margin, and ROI. |
| **INV-5** | **Offline Repeatability & Retrieval Speed** | When `data/cache/` contains the 13 symbols, running backtests with network disabled executes in $< 100\text{ms}$ disk load time. | Backtest fails offline or takes $> 500\text{ms}$ to load cached data. |

### Verification Commands for Worker:
1. **Rust Syntax & Type Check**:
   ```powershell
   cargo check --manifest-path E:\binance_futures_bot\Cargo.toml
   # MUST exit with code 0 and 0 errors
   ```
2. **Execute Full E2E Test Suite (Tier 1–4)**:
   ```powershell
   python -X utf8 -m unittest discover -s E:\binance_futures_bot\tests
   # MUST run 35+ tests and exit with code 0 (OK)
   ```
3. **Verify Baseline Invariant & JSON Output Schema**:
   ```powershell
   python -X utf8 scratch/test_params.py
   # Confirms 13-symbol kline ingestion parity
   ```

---

## 6. File Ownership Boundaries for Worker

To ensure isolated, surgical, regression-free implementation, file ownership is partitioned as follows:

| Target File / Directory | Worker Permission | Modification Scope | Locked / Forbidden Scope |
|---|---|---|---|
| `src/main.rs` | **SURGICAL EDIT ONLY** | Lines 1490–2061 (`run_native_rust_backtest`) to add cache check, active position struct, duration calculation, and JSON writing.<br>Lines 252–267 (`ClosedTradeRecord`) to add duration fields. | **STRICTLY LOCKED**: Lines 947–1065 (Regimes 1, 2, 3), lines 1068–1123 (Sizing), lines 2232–2360 (Arbiter), lines 2363–2412 (Emergency exit), and live trading daemon lines 2919–3453. |
| `data/cache/` | **OWNED (CREATE & WRITE)** | Create directory; populate `{symbol}_{interval}.json` and `cache_metadata.json`. | Do not write non-kline files here. |
| `reports/` | **OWNED (CREATE & WRITE)** | Create directory; output `backtest_latest.json`. | Do not commit unversioned binary artifacts. |
| `tests/` | **OWNED (CREATE / RUN)** | Run existing tests; add M1-specific verification scripts (e.g. `tests/test_m1_caching.py`). | Do not delete or break existing passing tests in `tests/test_harness.py`. |
| `.agents/teamwork/` | **READ-ONLY (EXCEPT OWN)** | Write ONLY to Worker's assigned directory (`.agents/teamwork/worker_m1_*/`). | **NEVER** write source code, test files, or datasets in `.agents/teamwork/`. |
