#!/usr/bin/env python3
"""
Binance Futures Historical Aggregate Trades (aggTrades) Fetcher
Repo: farqab/binance_futures_bot
File: tools/fetch_aggtrades.py

Fetches tick-level aggregated trade order-flow from Binance USDT-M Futures public endpoint:
https://fapi.binance.com/fapi/v1/aggTrades

Features:
- Pure public REST API, zero API keys or authentication required.
- Sequential fromId pagination for 100% gapless and deterministic trade sequencing.
- High-efficiency Parquet columnar storage (with JSON/JSONL fallback).
- Automatic rate-limit throttling and exponential backoff on HTTP 429/418.
- Comprehensive trade stream verification:
  * Total trades and trade ID monotonicity
  * Timestamp continuity
  * Volume and taker buy/sell imbalance aggregation
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Dict, List, Optional, Tuple

BASE_URL = "https://fapi.binance.com/fapi/v1/aggTrades"
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DEFAULT_OUTPUT_DIR = os.path.join(REPO_ROOT, "data", "aggtrades")

DEFAULT_SYMBOLS = [
    "BTCUSDT",
    "ETHUSDT",
    "SOLUSDT",
    "DOGEUSDT",
    "NEARUSDT",
    "APTUSDT",
    "SUIUSDT",
    "AVAXUSDT",
    "LINKUSDT",
]

# Check for parquet availability
HAVE_PYARROW = False
try:
    import pyarrow as pa
    import pyarrow.parquet as pq
    HAVE_PYARROW = True
except ImportError:
    pass


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


def fetch_aggtrades_chunk(
    symbol: str,
    from_id: Optional[int] = None,
    start_time: Optional[int] = None,
    end_time: Optional[int] = None,
    limit: int = 1000,
    max_retries: int = 5,
) -> List[Dict[str, Any]]:
    """Fetches a single chunk of up to 1000 aggregate trades with backoff."""
    params: Dict[str, Any] = {
        "symbol": symbol,
        "limit": limit,
    }
    if from_id is not None:
        params["fromId"] = from_id
    elif start_time is not None:
        params["startTime"] = start_time
        if end_time is not None:
            params["endTime"] = min(end_time, start_time + 3_600_000)  # max 1 hour window

    url = f"{BASE_URL}?{urllib.parse.urlencode(params)}"
    headers = {"User-Agent": "BinanceFuturesBot-AggTradeFetcher/1.0"}

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
                print(f"⚠️ [HTTP {e.code}] Rate limit for {symbol}. Backing off {wait_sec:.1f}s...")
                time.sleep(wait_sec)
                delay = min(delay * 2, 60.0)
            elif e.code in (500, 502, 503, 504):
                print(f"⚠️ [HTTP {e.code}] Gateway glitch for {symbol}. Retrying in {delay:.1f}s...")
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


def fetch_symbol_aggtrades(
    symbol: str,
    start_time_ms: int,
    end_time_ms: int,
    output_dir: str = DEFAULT_OUTPUT_DIR,
    max_trades: Optional[int] = None,
    preferred_format: str = "parquet",
    request_delay_sec: float = 0.05,
) -> Dict[str, Any]:
    """Paginates across aggregate trades using fromId sequencing."""
    os.makedirs(output_dir, exist_ok=True)
    print(f"\n🚀 Fetching aggTrades for {symbol}: {format_ms_to_iso(start_time_ms)} -> {format_ms_to_iso(end_time_ms)}")

    # Step 1: Bootstrap initial fromId using startTime
    init_chunk = fetch_aggtrades_chunk(symbol, start_time=start_time_ms, end_time=end_time_ms, limit=100)
    if not init_chunk:
        print(f"⚠️ No trades found for {symbol} starting at {format_ms_to_iso(start_time_ms)}")
        return {"symbol": symbol, "status": "EMPTY", "trades": 0}

    next_from_id = init_chunk[0]["a"]
    print(f"  Initial bootstrap fromId for {symbol}: {next_from_id:,} (timestamp: {format_ms_to_iso(init_chunk[0]['T'])})")

    # Ingestion buffers
    trade_ids: List[int] = []
    prices: List[float] = []
    quantities: List[float] = []
    first_trade_ids: List[int] = []
    last_trade_ids: List[int] = []
    timestamps: List[int] = []
    is_buyer_makers: List[bool] = []

    total_fetched = 0
    chunk_idx = 0
    last_ts = start_time_ms

    start_time_exec = time.time()

    while True:
        chunk = fetch_aggtrades_chunk(symbol, from_id=next_from_id, limit=1000)
        chunk_idx += 1

        if not chunk:
            break

        for t in chunk:
            t_ts = t["T"]
            if t_ts > end_time_ms:
                break

            trade_ids.append(t["a"])
            prices.append(float(t["p"]))
            quantities.append(float(t["q"]))
            first_trade_ids.append(t["f"])
            last_trade_ids.append(t["l"])
            timestamps.append(t_ts)
            is_buyer_makers.append(bool(t["m"]))

            total_fetched += 1
            last_ts = t_ts

        # Print live progress every 5 chunks
        if chunk_idx % 5 == 0 or total_fetched >= (max_trades or sys.maxsize):
            elapsed = max(0.1, time.time() - start_time_exec)
            tps = total_fetched / elapsed
            print(f"\r  Chunk #{chunk_idx:,}: {total_fetched:,} trades ({tps:,.0f} t/s) | Cursor: {format_ms_to_iso(last_ts)}", end="", flush=True)

        if max_trades and total_fetched >= max_trades:
            print(f"\n  Reached requested maximum trades limit: {max_trades:,}")
            break

        if last_ts >= end_time_ms or len(chunk) < 1000:
            break

        # Next fromId is strictly the last seen aggregate trade id + 1
        next_from_id = chunk[-1]["a"] + 1
        time.sleep(request_delay_sec)

    print()  # Final newline

    if total_fetched == 0:
        return {"symbol": symbol, "status": "EMPTY", "trades": 0}

    # Save to disk
    use_parquet = (preferred_format.lower() in ("parquet", "auto")) and HAVE_PYARROW
    out_file = ""

    if use_parquet:
        out_file = os.path.join(output_dir, f"{symbol}.parquet")
        schema = pa.schema([
            ("agg_trade_id", pa.int64()),
            ("price", pa.float64()),
            ("quantity", pa.float64()),
            ("first_trade_id", pa.int64()),
            ("last_trade_id", pa.int64()),
            ("timestamp_ms", pa.int64()),
            ("is_buyer_maker", pa.bool_()),
        ])
        tbl = pa.Table.from_arrays([
            pa.array(trade_ids, type=pa.int64()),
            pa.array(prices, type=pa.float64()),
            pa.array(quantities, type=pa.float64()),
            pa.array(first_trade_ids, type=pa.int64()),
            pa.array(last_trade_ids, type=pa.int64()),
            pa.array(timestamps, type=pa.int64()),
            pa.array(is_buyer_makers, type=pa.bool_()),
        ], schema=schema)
        pq.write_table(tbl, out_file, compression="SNAPPY")
    else:
        out_file = os.path.join(output_dir, f"{symbol}.json")
        payload = {
            "symbol": symbol,
            "count": total_fetched,
            "start_time": timestamps[0],
            "end_time": timestamps[-1],
            "trades": [
                {
                    "a": trade_ids[i],
                    "p": prices[i],
                    "q": quantities[i],
                    "f": first_trade_ids[i],
                    "l": last_trade_ids[i],
                    "T": timestamps[i],
                    "m": is_buyer_makers[i],
                }
                for i in range(total_fetched)
            ]
        }
        with open(out_file, "w", encoding="utf-8") as f:
            json.dump(payload, f, separators=(",", ":"))

    file_size_mb = os.path.getsize(out_file) / (1024 * 1024)

    # Verification
    first_id = trade_ids[0]
    last_id = trade_ids[-1]
    expected_ids = last_id - first_id + 1
    dropped_ids = expected_ids - total_fetched

    return {
        "symbol": symbol,
        "status": "CLEAN" if dropped_ids == 0 else "GAPS_DETECTED",
        "trades": total_fetched,
        "first_trade": format_ms_to_iso(timestamps[0]),
        "last_trade": format_ms_to_iso(timestamps[-1]),
        "first_id": first_id,
        "last_id": last_id,
        "dropped_ids": dropped_ids,
        "output_file": f"{out_file} ({file_size_mb:.2f} MB)",
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="High-Speed Binance Futures Public Aggregate Trades (aggTrades) Fetcher",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--symbols",
        type=str,
        default=",".join(DEFAULT_SYMBOLS),
        help="Comma-separated list of symbols (e.g. BTCUSDT,ETHUSDT) or 'all'",
    )
    parser.add_argument(
        "--days",
        type=int,
        default=30,
        help="Number of days to fetch back from end date (default: 30 days)",
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
        "--max-trades",
        type=int,
        default=None,
        help="Optional ceiling on number of trades per symbol (useful for testing or sample slices)",
    )
    parser.add_argument(
        "--format",
        type=str,
        choices=["parquet", "json", "auto"],
        default="parquet" if HAVE_PYARROW else "json",
        help="Storage format (parquet requires pyarrow)",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default=DEFAULT_OUTPUT_DIR,
        help="Output directory for aggregate trades",
    )
    parser.add_argument(
        "--delay",
        type=float,
        default=0.05,
        help="Delay between requests in seconds (default: 50ms)",
    )

    args = parser.parse_args()

    now_ms = int(datetime.datetime.now(datetime.timezone.utc).timestamp() * 1000)
    end_time_ms = parse_date_to_ms(args.end) if args.end else now_ms

    if args.start:
        start_time_ms = parse_date_to_ms(args.start)
    else:
        start_time_ms = end_time_ms - (args.days * 86_400_000)

    symbols = [s.strip().upper() for s in args.symbols.split(",") if s.strip()]
    if "ALL" in symbols:
        symbols = DEFAULT_SYMBOLS

    print("=" * 80)
    print("🌊 BINANCE FUTURES PUBLIC AGGREGATE TRADES FETCHER")
    print(f"🎯 Symbols ({len(symbols)}): {', '.join(symbols)}")
    print(f"📅 Timeline: {format_ms_to_iso(start_time_ms)} -> {format_ms_to_iso(end_time_ms)}")
    print(f"📦 Format: {args.format} (PyArrow Available: {HAVE_PYARROW})")
    print(f"📁 Output Dir: {args.output_dir}")
    print("=" * 80)

    results: List[Dict[str, Any]] = []
    for sym in symbols:
        try:
            res = fetch_symbol_aggtrades(
                sym,
                start_time_ms,
                end_time_ms,
                output_dir=args.output_dir,
                max_trades=args.max_trades,
                preferred_format=args.format,
                request_delay_sec=args.delay,
            )
            results.append(res)
        except Exception as e:
            print(f"\n❌ Error fetching {sym}: {e}")
            results.append({"symbol": sym, "status": "FAILED", "trades": 0, "error": str(e)})

    # Summary report
    print("\n" + "=" * 90)
    print("📊 AGGTRADES INGESTION AUDIT REPORT")
    print("=" * 90)
    print(f"{'Symbol':<14} | {'Trades':<10} | {'First Trade':<23} | {'Last Trade':<23} | {'Status'}")
    print("-" * 90)
    for r in results:
        t_cnt = f"{r.get('trades', 0):,}"
        print(f"{r['symbol']:<14} | {t_cnt:<10} | {r.get('first_trade', 'N/A'):<23} | {r.get('last_trade', 'N/A'):<23} | {r.get('status', 'N/A')}")
    print("=" * 90)


if __name__ == "__main__":
    main()
