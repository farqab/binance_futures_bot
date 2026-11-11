#!/usr/bin/env python3
"""
Production Strategy Execution & CLI Runner Engine
Binance Futures Bot — Time-Based ROI Optimization Framework

Delivers Milestone 4 (Requirement R3):
- Fully runnable standalone CLI interface to execute any of the 5 canonical strategies.
- Supports --strategy, --timeframe, --days, --refresh-cache, --symbols, --initial-balance, and --output-json.
- Automatically loads klines from data/cache/ (or fetches on miss), executes the simulation,
  logs trades, and displays comprehensive performance metrics to stdout and JSON.

Usage:
    python -X utf8 run_strategy.py --strategy scalping_15m
    python -X utf8 run_strategy.py --strategy tri_regime_time_decay_1h
    python -X utf8 run_strategy.py --strategy volatility_scaled_1h --days 10
    python -X utf8 run_strategy.py --strategy split_tp_1h
    python -X utf8 run_strategy.py --strategy tri_regime_baseline
    python -X utf8 run_strategy.py --help
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

# Ensure project root is in sys.path
PROJECT_ROOT = Path(__file__).resolve().parent
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from data.kline_cache import KlineCache, TradeRecord
from tests.test_harness import (
    CANONICAL_SYMBOLS,
    ConfigManager,
    RankingEngine,
    StrategyEngine,
    TimeDecayRule,
)
from strategies.time_decay_optimizer import StrategyPreset, TimeDecayOptimizer

CONFIG_FILE = PROJECT_ROOT / "config" / "strategy_config.json"
CACHE_DIR = PROJECT_ROOT / "data" / "cache"
REPORTS_DIR = PROJECT_ROOT / "reports"

STRATEGY_ALIASES: Dict[str, str] = {
    # scalping 15m
    "scalping_15m": "scalping_15m",
    "scalping": "scalping_15m",
    "15m": "scalping_15m",
    "4": "scalping_15m",
    "strategy_4": "scalping_15m",
    # tri_regime_time_decay_1h
    "tri_regime_time_decay_1h": "tri_regime_time_decay_1h",
    "time_decay_1h": "tri_regime_time_decay_1h",
    "time_decay": "tri_regime_time_decay_1h",
    "2": "tri_regime_time_decay_1h",
    "strategy_2": "tri_regime_time_decay_1h",
    # volatility_scaled_1h
    "volatility_scaled_1h": "volatility_scaled_1h",
    "volatility_scaled": "volatility_scaled_1h",
    "vol_scaled": "volatility_scaled_1h",
    "5": "volatility_scaled_1h",
    "strategy_5": "volatility_scaled_1h",
    # split_tp_1h
    "split_tp_1h": "split_tp_1h",
    "split_tp": "split_tp_1h",
    "3": "split_tp_1h",
    "strategy_3": "split_tp_1h",
    # tri_regime_baseline
    "tri_regime_baseline": "tri_regime_baseline",
    "baseline_1h": "tri_regime_baseline",
    "baseline": "tri_regime_baseline",
    "1": "tri_regime_baseline",
    "strategy_1": "tri_regime_baseline",
}


def load_strategy_config(strategy_key: str) -> Tuple[str, Dict[str, Any]]:
    """Loads and validates strategy configuration preset from config/strategy_config.json."""
    if not CONFIG_FILE.exists():
        raise FileNotFoundError(f"Configuration file not found: {CONFIG_FILE}")

    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        master_cfg = json.load(f)

    norm_key = strategy_key.strip().lower()
    canonical_key = STRATEGY_ALIASES.get(norm_key, norm_key)

    strategies_map = master_cfg.get("strategies", {})
    if canonical_key not in strategies_map:
        valid_keys = list(strategies_map.keys())
        raise ValueError(
            f"Unknown strategy '{strategy_key}'. Available strategies: {', '.join(valid_keys)}"
        )

    strat_cfg = strategies_map[canonical_key]
    ConfigManager.validate_config(strat_cfg)
    return canonical_key, strat_cfg


class StrategyRunner:
    """Production Strategy Execution Engine for backtests and forward simulations."""

    def __init__(
        self,
        strategy_key: str,
        timeframe_override: Optional[str] = None,
        days: Optional[int] = None,
        refresh_cache: bool = False,
        symbols: Optional[List[str]] = None,
        initial_balance: float = 10.0,
    ):
        self.strategy_key, self.config = load_strategy_config(strategy_key)
        self.timeframe = timeframe_override or self.config.get("timeframe", "1h")
        self.days = days
        self.refresh_cache = refresh_cache
        self.symbols = symbols or self.config.get("symbols", list(CANONICAL_SYMBOLS.keys()))
        self.initial_balance = initial_balance
        self.cache = KlineCache(CACHE_DIR)

    def prepare_data(self) -> Dict[str, List[Tuple]]:
        """Prepares dataset for execution, refreshing or fetching klines if needed."""
        dataset: Dict[str, List[Tuple]] = {}
        tf = self.timeframe

        for sym in self.symbols:
            cache_file = self.cache.get_cache_path(sym, tf)
            need_fetch = self.refresh_cache or (not cache_file.exists())

            if need_fetch:
                print(f"📥 Fetching remote klines for {sym} {tf} from Binance Futures REST API...")
                success = self.cache.populate_cache(sym, tf, target_bars=1500, verbose=False)
                if not success:
                    print(f"⚠️ Failed to fetch remote klines for {sym} {tf}, attempting disk fallback...")

            try:
                raw_bars = self.cache.load_klines(sym, tf, validate=False)
                bars = [tuple(b) for b in raw_bars]

                if self.days is not None and self.days > 0:
                    bars_per_day = 24 if tf == "1h" else (96 if tf == "15m" else 24)
                    bars_to_keep = self.days * bars_per_day
                    bars = bars[-bars_to_keep:]

                dataset[sym] = bars
            except Exception as e:
                print(f"⚠️ Warning: Could not load klines for {sym} {tf}: {e}")

        if not dataset:
            raise RuntimeError(f"No kline data available for timeframe {tf}")

        return dataset

    def run(self) -> Dict[str, Any]:
        """Executes the strategy backtest simulation and computes performance metrics."""
        dataset = self.prepare_data()

        # Build StrategyPreset
        preset = StrategyPreset(
            name=self.config.get("name", self.strategy_key),
            timeframe=self.timeframe,
            atr_mult_sl=float(self.config.get("atr_mult_sl", 1.8)),
            atr_mult_tp=float(self.config.get("atr_mult_tp", 3.6)),
            tighten_hours=float(self.config.get("time_decay_tighten_hours", 12.0)),
            hard_exit_hours=float(self.config.get("time_decay_hard_exit_hours", 24.0)),
            split_tp_enabled=bool(self.config.get("split_tp_enabled", False)),
            volatility_scaled=bool(self.config.get("volatility_scaled", False)),
            description=self.config.get("description", ""),
        )

        # Initialize Optimizer with loaded klines
        optimizer = TimeDecayOptimizer(cache_dir=CACHE_DIR)
        optimizer.symbols = [s for s in self.symbols if s in dataset]

        if self.timeframe == "15m":
            optimizer.klines_15m = dataset
        else:
            optimizer.klines_1h = dataset

        t0 = time.perf_counter()
        result = optimizer.evaluate_strategy(preset, initial_balance=self.initial_balance)
        exec_latency_ms = (time.perf_counter() - t0) * 1000

        result["strategy_key"] = self.strategy_key
        result["execution_latency_ms"] = round(exec_latency_ms, 2)
        result["executed_at_utc"] = datetime.now(timezone.utc).isoformat()
        result["timeframe"] = self.timeframe
        result["days_filtered"] = self.days
        result["symbols_evaluated"] = optimizer.symbols

        return result


def format_performance_table(result: Dict[str, Any]) -> str:
    """Formats metrics dictionary into an elegant ASCII/Markdown performance table."""
    m = result["metrics"]
    p = result.get("parameters", {})
    trades = result.get("trades", [])

    win_trades = sum(1 for t in trades if t["net_pnl"] > 0)
    loss_trades = sum(1 for t in trades if t["net_pnl"] <= 0)
    total_fees = sum(t["fees"] for t in trades)
    gross_pnl = sum(t["gross_pnl"] for t in trades)
    net_pnl = sum(t["net_pnl"] for t in trades)

    exits: Dict[str, int] = {}
    for t in trades:
        r = t.get("exit_reason", "UNKNOWN")
        exits[r] = exits.get(r, 0) + 1

    lines = [
        "┌" + "─" * 78 + "┐",
        f"│ {'PERFORMANCE METRICS SUMMARY':^76} │",
        "├" + "─" * 78 + "┤",
        f"│ Strategy Name:      {result.get('strategy_name', 'Unknown')[:56]:<56} │",
        f"│ Timeframe:          {result.get('timeframe', '1h'):<10} | Capital: ${m['initial_balance']:.2f} -> ${m['final_balance']:.2f}{'':<24} │",
        f"│ Net Profit / Loss:  {m['net_profit_pct']:+.2f}% ({net_pnl:+.4f} USD){'':<36} │",
        f"│ Gross PnL / Fees:   Gross: {gross_pnl:+.4f} USD | Total Fees: {total_fees:.4f} USD{'':<17} │",
        f"│ Win Rate:           {m['win_rate_pct']:.2f}% ({win_trades} Wins / {loss_trades} Losses){'':<32} │",
        f"│ Profit Factor:      {m['profit_factor']:.2f}{'':<58} │",
        f"│ Max Drawdown:       {m['max_drawdown_pct']:.2f}%{'':<57} │",
        f"│ Avg Trade Duration: {m['avg_duration_hours']:.2f} hours{'':<52} │",
        f"│ Hourly Active ROI:  {m['active_capital_hourly_roi']:+.4f}% / hour{'':<47} │",
        f"│ Time-Adjusted TAS:  {m['tas_score']:.2f}{'':<58} │",
        f"│ Sharpe / Calmar:    Sharpe: {m['sharpe_ratio']:.2f} | Calmar: {m['calmar_ratio']:.2f}{'':<35} │",
        "├" + "─" * 78 + "┤",
        f"│ {'EXIT REASON DISTRIBUTION':^76} │",
        "├" + "─" * 78 + "┤",
    ]

    for reason, count in sorted(exits.items()):
        pct = (count / max(len(trades), 1)) * 100
        lines.append(f"│  • {reason:<20}: {count:>3} trades ({pct:5.1f}%){'':<41} │")

    lines.append("└" + "─" * 78 + "┘")
    return "\n".join(lines)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Production Strategy Execution CLI Runner — Binance Futures Bot",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Available Strategies:
  scalping_15m               High-Velocity 15m Scalping with 6-bar decay
  tri_regime_time_decay_1h   Tri-Regime 1h with 12h BE tightening / 24h market exit
  volatility_scaled_1h       Dynamic ATR-scaled decay thresholds (TAS Rank 1)
  split_tp_1h                Split Take-Profit (50% at 1.8 ATR, 50% at 3.6 ATR)
  tri_regime_baseline        Standard 1h baseline without time-decay

Examples:
  python -X utf8 run_strategy.py --strategy scalping_15m
  python -X utf8 run_strategy.py --strategy tri_regime_time_decay_1h
  python -X utf8 run_strategy.py --strategy volatility_scaled_1h --days 14
  python -X utf8 run_strategy.py --strategy split_tp_1h --output-json reports/custom.json
        """,
    )
    parser.add_argument(
        "--strategy",
        "-s",
        type=str,
        default="scalping_15m",
        help="Strategy to execute (scalping_15m, tri_regime_time_decay_1h, volatility_scaled_1h, split_tp_1h, tri_regime_baseline). Default: scalping_15m.",
    )
    parser.add_argument(
        "--timeframe",
        "-t",
        type=str,
        choices=["1h", "15m", "5m"],
        default=None,
        help="Candle timeframe override (1h or 15m). If omitted, defaults to strategy preset timeframe.",
    )
    parser.add_argument(
        "--days",
        "-d",
        type=int,
        default=None,
        help="Limit backtest to the most recent N days of candles. If omitted, uses full cached dataset.",
    )
    parser.add_argument(
        "--refresh-cache",
        "-r",
        action="store_true",
        help="Force re-fetch/update klines from Binance Futures REST API.",
    )
    parser.add_argument(
        "--symbols",
        type=str,
        default=None,
        help="Comma-separated symbols to trade (e.g. BTCUSDT,ETHUSDT,SOLUSDT). Default: all 13 canonical pairs.",
    )
    parser.add_argument(
        "--initial-balance",
        "-b",
        type=float,
        default=10.0,
        help="Initial capital in USD (micro-capital default: $10.00).",
    )
    parser.add_argument(
        "--output-json",
        "-o",
        type=str,
        default=None,
        help="Custom file path for JSON output. Default: reports/run_strategy_<strategy>.json.",
    )
    parser.add_argument(
        "--quiet",
        "-q",
        action="store_true",
        help="Suppress trade event logging to stdout.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_arguments()

    symbols_list = None
    if args.symbols:
        symbols_list = [s.strip().upper() for s in args.symbols.split(",") if s.strip()]

    print("=" * 80)
    print("🚀 BINANCE FUTURES BOT — STRATEGY EXECUTION RUNNER")
    print(f"Target Strategy: {args.strategy}")
    if args.timeframe:
        print(f"Timeframe Override: {args.timeframe}")
    if args.days:
        print(f"Lookback Window: Last {args.days} days")
    print("=" * 80)

    try:
        runner = StrategyRunner(
            strategy_key=args.strategy,
            timeframe_override=args.timeframe,
            days=args.days,
            refresh_cache=args.refresh_cache,
            symbols=symbols_list,
            initial_balance=args.initial_balance,
        )

        result = runner.run()

        # Display performance table
        print("\n" + format_performance_table(result) + "\n")

        # Save JSON output
        out_file = args.output_json
        if not out_file:
            REPORTS_DIR.mkdir(parents=True, exist_ok=True)
            out_file = REPORTS_DIR / f"run_strategy_{runner.strategy_key}.json"
        else:
            out_file = Path(out_file)
            out_file.parent.mkdir(parents=True, exist_ok=True)

        with open(out_file, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2)

        print(f"📁 Detailed execution artifact written to: {out_file}")
        print(f"⚡ Execution latency: {result['execution_latency_ms']} ms")
        print("✅ Strategy execution completed successfully.\n")
        return 0

    except Exception as e:
        print(f"\n❌ Error executing strategy '{args.strategy}': {e}\n", file=sys.stderr)
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
