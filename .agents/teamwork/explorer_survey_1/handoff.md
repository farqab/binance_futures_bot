# Handoff Report — explorer_survey_1

**Task:** Survey of Existing Backtest Records, Historical Logs, Data Feeds, Performance Metrics, and Evaluation Scripts in `E:\binance_futures_bot`.  
**Agent:** `explorer_survey_1` (teamwork_explorer)  
**Parent:** `4cd38888-59b8-4580-a42f-c9475ff72425`  
**Date:** 2026-11-06T16:41:00Z  

---

## 1. Observation

1. **Filesystem Scan for Backtest Data Artifacts**:
   - Running `find_by_name` across `E:\binance_futures_bot` excluding `target/` for extensions `[csv, json, db, sqlite, log]` yielded exactly two files:
     ```
     bot_state.json
     testnet_keys.json
     ```
   - No historical `.csv`, `.parquet`, `.sqlite`, or `.log` backtest result files exist anywhere in the repository.
   - `bot_state.json` (lines 1–17) stores live production state with `engine_version: "v5.0-DUAL-ENGINE-MAINNET"`, `balance: 9.88`, and empty `closed_trades: []`.

2. **Backtest Implementation in Rust**:
   - `src/main.rs:1490–2061` implements `run_native_rust_backtest(args: &[String]) -> Result<()>`.
   - Lines 1528–1536 show live HTTP fetching via `curl.exe`:
     ```rust
     let url1 = format!(
         "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval={}&limit=1500",
         spec.symbol, tf
     );
     if let Ok(output) = Command::new(CURL_BIN)
         .args(["-s", "--max-time", "15", &url1])
         .output()
     ```
   - Lines 1994–2058 show all backtest outputs are printed directly to STDOUT using `println!`. No files are written.

3. **Binary vs. Source Code Version Drift**:
   - File modification timestamps:
     - `src/main.rs`: LastWriteTime `6.11.2026 18:35:10` (Length: 136,987 bytes).
     - `target/debug/binance_futures_bot.exe`: LastWriteTime `4.11.2026 23:02:13` (Length: 23,022,873 bytes).
   - Executing `E:\binance_futures_bot\target\debug\binance_futures_bot.exe --backtest --10d` returned:
     ```
     📅 NATIVE RUST 15m GÜNLÜK TAKVİM BACKTEST & DOĞRULAMA MOTORU (v5.0 Dual-Engine)
     📅 GÜNLÜK TAKVİM DÖKÜMÜ (15 Dakikalık Mumlar - 6 Coin Birleşik):
     ...
     🏆 [GENEL TOPLAM (11 Takvim Günü | 1499x6=8994 Mum)]
        Başlangıç: $10.00 -> Son Kasa: $14.97 (+49.74%) | Ortalama Günlük Getiri: +0.45$ (+4.52% / gün)
        Toplam İşlem: 25 (15 Kazanan / 10 Kaybeden -> WinRate: %60.0) | Profit Factor: 1.66 | Toplam Fee: -$2.04 | Max DD: %16.75
     ```
   - In contrast, `src/main.rs:1509–1510` defines:
     ```rust
     println!("📅 NATIVE RUST 1h GÜNLÜK TAKVİM BACKTEST MOTORU (Institutional Liquidity Sweep & Wick Exhaustion v7.1)");
     println!("Kapsam: {} Gün ({} Saat) (13 Parite)...");
     ```
   - The compiled binary reflects the older November 4 state (6 coins, 15m candles, v5.0), whereas `src/main.rs` contains the November 6 state (13 coins, 1h candles, v7.5/v8.3).

4. **Compiler / Linker Status**:
   - Running `cargo check --manifest-path E:\binance_futures_bot\Cargo.toml` exited with code 0 (clean check, 6 minor unused warnings).
   - Running `cargo build` failed at linking:
     ```
     error: linking with `x86_64-w64-mingw32-gcc` failed: exit code: 1
     ld: cannot find crt2.o: No such file or directory
     ld: cannot find -lkernel32: No such file or directory
     ```

5. **Python Verification of Binance Kline Feed**:
   - Executing `python -X utf8 E:\binance_futures_bot\scratch\test_params.py` completed in 6.0 seconds:
     ```
     Fetching data for 13 symbols...
       ETHUSDT: 1499 bars
       XRPUSDT: 1499 bars
       ...
       TIAUSDT: 1499 bars
     Done. 13 symbols fetched.
     ```

6. **Metrics Calculated in Backtest (`src/main.rs:2029–2058`)**:
   - Calculated: Total Trades (`wins + losses`), Wins, Losses, Win Rate (`wins/total`), Profit Factor (`gross_win / gross_loss`), Total Fees (`total_fees`), Max Drawdown (`max_dd`), Final Balance, Net Return %, Daily Net PnL, Daily Return %.
   - Omitted: Holding duration (`time_in_trade`), Time-weighted ROI (`ROI / time`), Sharpe ratio, Sortino ratio, Calmar ratio, trade expectancy, and per-coin performance breakdown.

---

## 2. Logic Chain

1. From **Observation 1**, there are no local historical dataset files or persisted backtest outputs in `E:\binance_futures_bot`. Therefore, historical backtest evaluations do not run against a fixed static local file, but depend on external API requests.
2. From **Observation 2**, `run_native_rust_backtest` downloads candles dynamically from Binance Futures REST API and prints formatted results strictly to standard output. Thus, downstream optimization or comparative ranking cannot directly read from existing backtest logs; a structured data output mechanism (JSON/CSV) must be introduced.
3. From **Observation 3**, the existing binary in `target/debug` was compiled on Nov 4, 2026, and executes an outdated v5.0 engine over 6 coins on 15m candles, while `src/main.rs` was updated on Nov 6, 2026, to an expanded 13-coin v7.5/v8.3 engine on 1h candles. Evaluating strategies via the pre-compiled binary would produce stale, unrepresentative results.
4. From **Observation 4**, the host environment's MinGW GCC linker cannot find C runtime files (`crt2.o`, `kernel32`), preventing direct compilation of the updated Rust binary on Windows without linker reconfiguration. However, `cargo check` confirms the Rust syntax and type-checking are 100% valid.
5. From **Observation 5**, Python scripts execute without friction, can fetch 1,499 bars of 1h historical klines across all 13 symbols in 6 seconds, and can easily compute quantitative metrics.
6. From **Observation 6**, the current engine tracks standard accounting metrics (WinRate, PF, Net PnL, Drawdown), but completely lacks time-in-trade duration tracking, time-weighted ROI, and risk-adjusted metrics (Sharpe, Sortino, Calmar). To fulfill Requirements R1 and R2 of the original request ("zaman bazlı kârı maksimize eden, ROI / birim zaman"), the evaluation pipeline must explicitly measure and optimize for trade duration and time-efficiency.

---

## 3. Caveats

1. **Historical Order Flow Approximation**: Binance historical klines (`/fapi/v1/klines`) provide OHLCV and Taker Buy Volume, but do not provide historical 5-minute Delta Open Interest (OI) or historical Top Trader Long/Short ratios. In backtesting, `SymbolEngine` uses available kline volume and simulated/current ratios for order flow confluence.
2. **Serial Execution Limitation**: In `src/main.rs`, `active: Option<...>` allows only one trade at a time across all 13 coins. This serial constraint means performance is heavily dependent on trade arrival order.
3. **Network Dependency**: Because historical data is fetched live from Binance, running backtests requires an active internet connection and is subject to Binance REST API availability.

---

## 4. Conclusion

1. **State of Backtest Records**: No persistent backtest logs, databases, or historical CSV/JSON files currently exist in the repository. All backtesting is dynamically fetched from Binance and emitted to stdout.
2. **Current Metrics Scope**: Standard metrics (WinRate: 60%, Profit Factor: 1.66, Net Return: +49.74%, Max DD: 16.75%, Total Fees: $2.04 over 11 days on the v5.0 6-coin test) are computed, but **time-based capital efficiency (ROI per hour/day) and risk-adjusted ratios (Sharpe, Sortino) are entirely missing**.
3. **Immediate Operational Requirement**: For Milestone 1 and 2, an ingestion and caching layer (e.g. saving the 1,499 klines per symbol to local JSON/CSV) along with an enhanced evaluation script that computes `time_in_trade`, `ROI_per_hour`, `Sharpe`, and per-coin attribution is required to benchmark and rank strategies systematically.

---

## 5. Verification Method

To independently verify the observations and findings in this report, run the following commands:

1. **Verify Absence of Local Backtest Logs / Datasets**:
   ```powershell
   Get-ChildItem -Path E:\binance_futures_bot -Recurse -Include *.csv, *.parquet, *.sqlite, *.db -Exclude target
   # Expected result: No items found
   ```

2. **Verify Binary vs Source Code Timestamp Mismatch**:
   ```powershell
   Get-Item E:\binance_futures_bot\src\main.rs, E:\binance_futures_bot\target\debug\binance_futures_bot.exe | Select-Object Name, Length, LastWriteTime
   # Expected result: main.rs is Nov 6 2026, binary is Nov 4 2026
   ```

3. **Verify Existing Pre-compiled Binary Execution**:
   ```powershell
   E:\binance_futures_bot\target\debug\binance_futures_bot.exe --backtest --10d
   # Expected result: Prints v5.0 6-coin 15m calendar backtest to stdout with WinRate 60.0% and Profit Factor 1.66
   ```

4. **Verify Live Kline Ingestion Speed in Python**:
   ```powershell
   python -X utf8 E:\binance_futures_bot\scratch\test_params.py
   # Expected result: Fetches 1,499 1h bars for all 13 symbols in ~6 seconds
   ```

5. **Verify Rust Syntax and Type Validity**:
   ```powershell
   cargo check --manifest-path E:\binance_futures_bot\Cargo.toml
   # Expected result: Finished dev profile with 0 errors (6 minor warnings)
   ```
