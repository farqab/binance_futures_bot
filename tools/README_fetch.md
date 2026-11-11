# Binance Futures Historical Data Ingestion & Validation Toolkit

High-speed, zero-authentication public data fetchers for USDT-M Binance Futures.  
Designed for statistical rigor, deterministic backtesting, and quantitative research.

---

## 📌 Architecture & Design Principles

1. **Zero-Authentication Invariant:** Uses Binance USDT-M Futures **public endpoints** (`/fapi/v1/klines` and `/fapi/v1/aggTrades`). No API keys, secrets, or IP whitelisting required.
2. **Schema Uniformity:** Output files (`data/cache/{SYMBOL}_{INTERVAL}.json`) strictly follow the production cache schema required by the Rust native backtest engine and `run_strategy.py`.
3. **Data Integrity & Gap Auditing:** Automatically inspects every retrieved sequence for timestamp gaps, missing candles, and duplicates.
4. **Adaptive Rate-Limiting:** Incorporates proactive inter-request delays and automatic exponential backoff with `Retry-After` header support for HTTP 429/418 responses.
5. **Git Cleanliness:** Large data cache files (`*_1m.json`, `*_5m.json`, `data/aggtrades/`, `*.parquet`) are strictly ignored via `.gitignore`.

---

## 🛠️ Tool 1: `fetch_klines.py` (Candle Data Fetcher)

Paginates historical OHLCV klines up to 1,500 bars per request across any canonical timeframe.

### Supported Symbols (13 Pairs)
`BTCUSDT`, `ETHUSDT`, `SOLUSDT`, `BNBUSDT`, `DOGEUSDT`, `NEARUSDT`, `APTUSDT`, `SUIUSDT`, `INJUSDT`, `AVAXUSDT`, `LINKUSDT`, `ARBUSDT`, `1000PEPEUSDT`

### CLI Arguments

| Argument | Default | Description |
| :--- | :--- | :--- |
| `--symbols` | 13 default pairs | Comma-separated symbol list (e.g. `BTCUSDT,ETHUSDT`) or `all` |
| `--timeframes`, `--intervals` | `5m` | Comma-separated timeframes (e.g. `1m`, `5m`, `15m`, `1h`) |
| `--days` | `180` | Number of calendar days to fetch backwards (default: 6 months) |
| `--start` | `None` | Explicit start date (`YYYY-MM-DD` or ISO timestamp). Overrides `--days`. |
| `--end` | `None` | Explicit end date (`YYYY-MM-DD` or ISO timestamp). Defaults to current UTC. |
| `--output-dir` | `data/cache` | Directory to store JSON cache files. |
| `--delay` | `0.08` | Delay between HTTP requests in seconds (80ms). |

### Example Commands

```bash
# 1. Fetch 6 months of 5m candles for all 13 symbols
python -X utf8 tools/fetch_klines.py --timeframes 5m --days 180

# 2. Fetch 6 months of 1m candles for top 3 symbols
python -X utf8 tools/fetch_klines.py --symbols BTCUSDT,ETHUSDT,SOLUSDT --timeframes 1m --days 180

# 3. Fetch both 1m and 5m candles for a specific historical date window
python -X utf8 tools/fetch_klines.py --symbols BTCUSDT --timeframes 1m,5m --start 2026-05-01 --end 2026-11-01

# 4. Quick sample check (last 7 days of 5m candles)
python -X utf8 tools/fetch_klines.py --symbols BTCUSDT,ETHUSDT --timeframes 5m --days 7
```

### Kline Output Format (`data/cache/{SYM}_{tf}.json`)
```json
{
  "schema_version": "1.0",
  "symbol": "BTCUSDT",
  "interval": "5m",
  "interval_ms": 300000,
  "bar_count": 51840,
  "start_time": 1789084800000,
  "end_time": 1794388800000,
  "fetched_at_utc": "2026-11-11T12:00:00+00:00",
  "checksum_sha256": "3a8f...92c1",
  "columns": [
    "open_time", "open", "high", "low", "close", "volume",
    "close_time", "quote_volume", "trades", "taker_buy_base_vol", "taker_buy_quote_vol"
  ],
  "data": [
    [1789084800000, 62450.1, 62510.0, 62390.5, 62480.0, 312.45, 1789085099999, 19521340.5, 8420, 160.2, 10008450.2]
  ],
  "klines": [ ... ]
}
```

---

## 🌊 Tool 2: `fetch_aggtrades.py` (Aggregate Order Flow Fetcher)

Paginates tick-level aggregated trades using deterministic `fromId` sequencing to ensure **zero missing trades and zero duplicate fills**.

### Supported Symbols (9 Pairs)
`BTCUSDT`, `ETHUSDT`, `SOLUSDT`, `DOGEUSDT`, `NEARUSDT`, `APTUSDT`, `SUIUSDT`, `AVAXUSDT`, `LINKUSDT`

### CLI Arguments

| Argument | Default | Description |
| :--- | :--- | :--- |
| `--symbols` | 9 default pairs | Comma-separated symbols or `all` |
| `--days` | `30` | Number of days back to ingest (default: 30 days) |
| `--start` | `None` | Explicit start date (`YYYY-MM-DD` or ISO timestamp). Overrides `--days`. |
| `--end` | `None` | Explicit end date (`YYYY-MM-DD` or ISO timestamp). Defaults to current UTC. |
| `--format` | `parquet` | Storage format: `parquet` (requires `pyarrow`) or `json` |
| `--max-trades` | `None` | Optional ceiling on trades per symbol (e.g. `50000` for testing) |
| `--output-dir` | `data/aggtrades` | Target storage directory |
| `--delay` | `0.05` | Delay between HTTP requests in seconds (50ms). |

### Example Commands

```bash
# 1. Fetch last 30 days of aggTrades in compressed Parquet format
python -X utf8 tools/fetch_aggtrades.py --symbols BTCUSDT --days 30 --format parquet

# 2. Fetch sample slice of 50,000 trades for order-flow testing
python -X utf8 tools/fetch_aggtrades.py --symbols BTCUSDT,ETHUSDT,SOLUSDT --days 7 --max-trades 50000

# 3. Fetch aggTrades in JSON format (fallback)
python -X utf8 tools/fetch_aggtrades.py --symbols NEARUSDT --days 3 --format json
```

### Parquet Schema (`data/aggtrades/{SYM}.parquet`)
* `agg_trade_id` (`int64`): Unique sequential aggregate trade ID.
* `price` (`float64`): Execution price.
* `quantity` (`float64`): Aggregate executed base quantity.
* `first_trade_id` (`int64`): First individual trade ID in the aggregation.
* `last_trade_id` (`int64`): Last individual trade ID in the aggregation.
* `timestamp_ms` (`int64`): Epoch timestamp in milliseconds.
* `is_buyer_maker` (`bool`): `True` if seller was taker (sell market order), `False` if buyer was taker (buy market order).

---

## 🛡️ Verification & Audit Guarantees

Both scripts print a post-run summary matrix verifying:
1. **Total Count:** Compares actual bar count against theoretical span $T_{\text{span}} / \Delta t$.
2. **Gap Detection:** Flags any interval where $t_{i+1} - t_i > \text{interval\_ms}$.
3. **Duplicate Check:** Confirms that $\text{Unique}(t) \equiv \text{Count}(t)$.
4. **Clean Exit:** Reports status as `CLEAN` only when zero gaps and zero duplicates are present.
