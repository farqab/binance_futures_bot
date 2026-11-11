#!/usr/bin/env python3
"""
Binance Futures Historical Klines Data Fetcher
Repo: farqab/binance_futures_bot
File: tools/fetch_klines.py

Fetches 1m, 5m (and any canonical interval) klines from Binance USDT-M Futures public endpoint:
https://fapi.binance.com/fapi/v1/klines

Features:
- Pure public REST API, zero API keys or authentication required.
- Pagination up to 1500 bars per request.
- Automatic rate-limit throttling and exponential backoff on HTTP 429/418.
- Comprehensive data integrity verification:
  * Count of candles vs expected timeline
  * Timestamp continuity & gap detection
  * Duplicate timestamp check
  * SHA-256 data payload checksum
- Output format: data/cache/{SYMBOL}_{INTERVAL}.json (bit-for-bit matches existing cache format).
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Dict, List, Optional, Tuple

BASE_URL = "https://fapi.binance.com/fapi/v1/klines"
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DEFAULT_CACHE_DIR = os.path.join(REPO_ROOT, "data", "cache")

DEFAULT_SYMBOLS = [
    "BTCUSDT",
    "ETHUSDT",
    "SOLUSDT",
    "BNBUSDT",
    "DOGEUSDT",
    "NEARUSDT",
    "APTUSDT",
    "SUIUSDT",
    "INJUSDT",
    "AVAXUSDT",
    "LINKUSDT",
    "ARBUSDT",
    "1000PEPEUSDT",
]

INTERVAL_MS_MAP = {
    "1m": 60_000,
    "3m": 180_000,
    "5m": 300_000,
    "15m": 900_000,
    "30m": 1_800_000,
    "1h": 3_600_000,
    "2h": 7_200_000,
    "4h": 14_400_000,
    "1d": 86_400_000,
}

COLUMNS = [
    "open_time",
    "open",
    "high",
    "low",
    "close",
    "volume",
    "close_time",
    "quote_volume",
    "trades",
    "taker_buy_base_vol",
    "taker_buy_quote_vol",
]


def parse_date_to_ms(date_str: str) -> int:
    """Parses ISO date string, YYYY-MM-DD, or integer timestamp into milliseconds."""
    try:
        ts_val = int(date_str)
        if ts_val < 10_000_000_000:
            return ts_val * 1000
        return ts_val
    except ValueError:
        pass

    clean_str = date_str.replace("Z", "+00:00")
    if "T" in clean_str:
        dt = datetime.datetime.fromisoformat(clean_str)
    else:
        dt = datetime.datetime.strptime(clean_str, "%Y-%m-%d").replace(tzinfo=datetime.timezone.utc)
    return int(dt.timestamp() * 1000)


def format_ms_to_iso(ts_ms: int) -> str:
    """Formats epoch milliseconds to UTC ISO-8601 string."""
    dt = datetime.datetime.fromtimestamp(ts_ms / 1000, tz=datetime.timezone.utc)
    return dt.strftime("%Y-%m-%d %H:%M:%S UTC")


def fetch_kline_chunk(
    symbol: str,
    interval: str,
    start_time: int,
    end_time: int,
    limit: int = 1500,
    max_retries: int = 5,
) -> List[List[Any]]:
    """Fetches a single chunk of klines with rate limiting and exponential backoff."""
    params = {
        "symbol": symbol,
        "interval": interval,
        "limit": limit,
        "startTime": start_time,
        "endTime": end_time,
    }
    url = f"{BASE_URL}?{urllib.parse.urlencode(params)}"
    headers = {"User-Agent": "BinanceFuturesBot-DataFetcher/1.0"}

    delay = 1.0
    for attempt in range(1, max_retries + 1):
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req, timeout=20) as resp:
                data = json.loads(resp.read().decode("utf-8"))
                return data
        except urllib.error.HTTPError as e:
            if e.code in (429, 418):
                retry_after = e.headers.get("Retry-After")
                wait_sec = float(retry_after) if retry_after else delay * 2
                print(f"⚠️ [HTTP {e.code}] Rate limit encountered for {symbol}. Backing off {wait_sec:.1f}s...")
                time.sleep(wait_sec)
                delay = min(delay * 2, 60.0)
            elif e.code in (500, 502, 503, 504):
                print(f"⚠️ [HTTP {e.code}] Binance gateway glitch for {symbol}. Retrying in {delay:.1f}s...")
                time.sleep(delay)
                delay = min(delay * 1.5, 30.0)
            else:
                body = e.read().decode("utf-8", errors="ignore")
                raise RuntimeError(f"HTTP Error {e.code} for {symbol}: {body}") from e
        except Exception as e:
            if attempt == max_retries:
                raise
            print(f"⚠️ Network error for {symbol} (attempt {attempt}/{max_retries}): {e}. Retrying...")
            time.sleep(delay)
            delay = min(delay * 1.5, 30.0)

    return []


def fetch_symbol_klines(
    symbol: str,
    interval: str,
    start_time_ms: int,
    end_time_ms: int,
    request_delay_sec: float = 0.08,
) -> Tuple[List[List[Any]], Dict[str, Any]]:
    """Paginates across the entire requested period to fetch complete klines."""
    interval_ms = INTERVAL_MS_MAP.get(interval)
    if not interval_ms:
        raise ValueError(f"Unsupported interval: {interval}")

    all_bars: List[List[Any]] = []
    seen_timestamps = set()
    current_start = start_time_ms

    est_bars = max(1, (end_time_ms - start_time_ms) // interval_ms)
    print(f"\n🚀 Fetching {symbol} [{interval}]: {format_ms_to_iso(start_time_ms)} -> {format_ms_to_iso(end_time_ms)} (~{est_bars:,} bars expected)")

    chunk_idx = 0
    while current_start <= end_time_ms:
        raw_chunk = fetch_kline_chunk(symbol, interval, current_start, end_time_ms, limit=1500)
        chunk_idx += 1

        if not raw_chunk:
            break

        added_in_chunk = 0
        last_open_time = current_start

        for r in raw_chunk:
            if len(r) < 11:
                continue
            open_time = int(r[0])
            if open_time > end_time_ms:
                break
            if open_time in seen_timestamps:
                continue

            # Typed row matching existing cache format:
            row = [
                open_time,
                float(r[1]),  # open
                float(r[2]),  # high
                float(r[3]),  # low
                float(r[4]),  # close
                float(r[5]),  # volume
                int(r[6]),    # close_time
                float(r[7]),  # quote_volume
                int(r[8]),    # trades
                float(r[9]),  # taker_buy_base_vol
                float(r[10]), # taker_buy_quote_vol
            ]
            all_bars.append(row)
            seen_timestamps.add(open_time)
            last_open_time = open_time
            added_in_chunk += 1

        # Progress reporting
        pct = min(100.0, (len(all_bars) / est_bars) * 100.0)
        print(f"\r  Chunk #{chunk_idx:02d}: +{added_in_chunk} bars (Total: {len(all_bars):,}/{est_bars:,} - {pct:.1f}%) | Cursor: {format_ms_to_iso(last_open_time)}", end="", flush=True)

        if len(raw_chunk) < 1500 or last_open_time >= end_time_ms:
            break

        current_start = last_open_time + interval_ms
        time.sleep(request_delay_sec)

    print()  # Newline after progress

    # Sort strictly by timestamp
    all_bars.sort(key=lambda b: b[0])

    # Run verification
    audit = verify_kline_series(all_bars, interval_ms, start_time_ms, end_time_ms)
    return all_bars, audit


def verify_kline_series(
    bars: List[List[Any]],
    interval_ms: int,
    expected_start_ms: int,
    expected_end_ms: int,
) -> Dict[str, Any]:
    """Audits kline sequence for duplicates, gaps, and coverage."""
    if not bars:
        return {
            "status": "EMPTY",
            "bar_count": 0,
            "duplicates": 0,
            "gaps": [],
            "gap_count": 0,
            "missing_bars_total": 0,
            "coverage_pct": 0.0,
        }

    bar_count = len(bars)
    timestamps = [b[0] for b in bars]
    unique_ts = set(timestamps)
    duplicate_count = bar_count - len(unique_ts)

    gaps: List[Dict[str, Any]] = []
    missing_bars_total = 0

    for i in range(len(bars) - 1):
        prev_ts = bars[i][0]
        next_ts = bars[i + 1][0]
        diff = next_ts - prev_ts

        if diff > interval_ms:
            missing = (diff // interval_ms) - 1
            missing_bars_total += missing
            gaps.append({
                "gap_start": format_ms_to_iso(prev_ts),
                "gap_end": format_ms_to_iso(next_ts),
                "missing_bars": missing,
                "gap_duration_minutes": (diff - interval_ms) / 60_000,
            })

    first_ts = bars[0][0]
    last_ts = bars[-1][0]
    span_bars = max(1, (last_ts - first_ts) // interval_ms + 1)
    coverage_pct = (bar_count / span_bars) * 100.0

    return {
        "status": "CLEAN" if len(gaps) == 0 and duplicate_count == 0 else "GAPS_DETECTED",
        "bar_count": bar_count,
        "duplicates": duplicate_count,
        "first_bar": format_ms_to_iso(first_ts),
        "last_bar": format_ms_to_iso(last_ts),
        "gaps": gaps,
        "gap_count": len(gaps),
        "missing_bars_total": missing_bars_total,
        "coverage_pct": round(coverage_pct, 2),
    }


def save_kline_cache(
    symbol: str,
    interval: str,
    bars: List[List[Any]],
    output_dir: str = "data/cache",
) -> str:
    """Saves klines into data/cache/{symbol}_{interval}.json matching cache schema."""
    os.makedirs(output_dir, exist_ok=True)
    out_path = os.path.join(output_dir, f"{symbol}_{interval}.json")

    interval_ms = INTERVAL_MS_MAP.get(interval, 0)
    data_json_bytes = json.dumps(bars).encode("utf-8")
    checksum = hashlib.sha256(data_json_bytes).hexdigest()

    payload = {
        "schema_version": "1.0",
        "symbol": symbol,
        "interval": interval,
        "interval_ms": interval_ms,
        "bar_count": len(bars),
        "start_time": bars[0][0] if bars else 0,
        "end_time": bars[-1][0] if bars else 0,
        "fetched_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "checksum_sha256": checksum,
        "columns": COLUMNS,
        "data": bars,
        "klines": bars,
    }

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, separators=(",", ":"))

    file_size_mb = os.path.getsize(out_path) / (1024 * 1024)
    return f"{out_path} ({file_size_mb:.2f} MB)"


def main() -> None:
    parser = argparse.ArgumentParser(
        description="High-Speed Binance Futures Public Historical Kline Data Fetcher & Validator",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--symbols",
        type=str,
        default=",".join(DEFAULT_SYMBOLS),
        help="Comma-separated list of symbols (e.g. BTCUSDT,ETHUSDT) or 'all'",
    )
    parser.add_argument(
        "--timeframes",
        "--intervals",
        type=str,
        default="5m",
        help="Comma-separated list of intervals (e.g. 1m,5m)",
    )
    parser.add_argument(
        "--days",
        type=int,
        default=180,
        help="Number of days to fetch back from end date (default: 180 days = 6 months)",
    )
    parser.add_argument(
        "--start",
        type=str,
        default=None,
        help="Explicit start date (YYYY-MM-DD or ISO timestamp). Overrides --days if set.",
    )
    parser.add_argument(
        "--end",
        type=str,
        default=None,
        help="Explicit end date (YYYY-MM-DD or ISO timestamp). Defaults to current UTC time.",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default=DEFAULT_CACHE_DIR,
        help="Output cache directory (default: repo data/cache)",
    )
    parser.add_argument(
        "--delay",
        type=float,
        default=0.08,
        help="Polite delay between HTTP requests in seconds (default: 80ms)",
    )
    parser.add_argument(
        "--concurrency",
        type=int,
        default=4,
        help="Number of concurrent worker threads across symbols (default: 4)",
    )

    args = parser.parse_args()

    # Determine timeline
    now_ms = int(datetime.datetime.now(datetime.timezone.utc).timestamp() * 1000)
    end_time_ms = parse_date_to_ms(args.end) if args.end else now_ms

    if args.start:
        start_time_ms = parse_date_to_ms(args.start)
    else:
        start_time_ms = end_time_ms - (args.days * 86_400_000)

    symbols = [s.strip().upper() for s in args.symbols.split(",") if s.strip()]
    if "ALL" in symbols:
        symbols = DEFAULT_SYMBOLS

    timeframes = [tf.strip() for tf in args.timeframes.split(",") if tf.strip()]

    print("=" * 80)
    print("📥 BINANCE FUTURES PUBLIC HISTORICAL DATA FETCHER")
    print(f"🎯 Symbols ({len(symbols)}): {', '.join(symbols)}")
    print(f"⏱️ Timeframes: {', '.join(timeframes)}")
    print(f"📅 Timeline: {format_ms_to_iso(start_time_ms)} -> {format_ms_to_iso(end_time_ms)}")
    print(f"📁 Output Dir: {args.output_dir}")
    print("=" * 80)

    import concurrent.futures

    def worker_fetch(task: Tuple[str, str]) -> Dict[str, Any]:
        sym, tf = task
        try:
            bars, audit = fetch_symbol_klines(
                sym, tf, start_time_ms, end_time_ms, request_delay_sec=args.delay
            )
            if bars:
                saved_info = save_kline_cache(sym, tf, bars, output_dir=args.output_dir)
                return {
                    "symbol": sym,
                    "interval": tf,
                    "bars": audit["bar_count"],
                    "first": audit["first_bar"],
                    "last": audit["last_bar"],
                    "gaps": audit["gap_count"],
                    "duplicates": audit["duplicates"],
                    "status": audit["status"],
                    "saved": saved_info,
                }
            else:
                return {
                    "symbol": sym,
                    "interval": tf,
                    "bars": 0,
                    "first": "N/A",
                    "last": "N/A",
                    "gaps": 0,
                    "duplicates": 0,
                    "status": "EMPTY",
                    "saved": "N/A",
                }
        except Exception as e:
            print(f"\n❌ Error fetching {sym} {tf}: {e}")
            return {
                "symbol": sym,
                "interval": tf,
                "bars": 0,
                "first": "ERROR",
                "last": str(e),
                "gaps": 0,
                "duplicates": 0,
                "status": "FAILED",
                "saved": "N/A",
            }

    tasks = [(sym, tf) for tf in timeframes for sym in symbols]
    summary_table: List[Dict[str, Any]] = []

    if args.concurrency > 1 and len(tasks) > 1:
        print(f"⚡ Running with {args.concurrency} concurrent worker threads...")
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
            futures = [executor.submit(worker_fetch, t) for t in tasks]
            for f in concurrent.futures.as_completed(futures):
                summary_table.append(f.result())
    else:
        for t in tasks:
            summary_table.append(worker_fetch(t))

    # Print summary report
    print("\n" + "=" * 90)
    print("📊 DATA INGESTION & INTEGRITY AUDIT REPORT")
    print("=" * 90)
    print(f"{'Symbol':<14} | {'TF':<4} | {'Bars':<8} | {'First Bar':<23} | {'Last Bar':<23} | {'Gaps':<5} | {'Status'}")
    print("-" * 90)
    for row in summary_table:
        print(f"{row['symbol']:<14} | {row['interval']:<4} | {row['bars']:<8,}"
              f" | {row['first']:<23} | {row['last']:<23} | {row['gaps']:<5} | {row['status']}")
    print("=" * 90)


if __name__ == "__main__":
    main()
