"""
Milestone 2 Feature 4: Time-Weighted Multi-Strategy Optimization Framework.
Evaluates and benchmarks all 5 canonical strategies across 13 cached crypto pairs on Binance Futures.

Path: strategies/time_decay_optimizer.py
"""

from __future__ import annotations

import json
import math
import sys
import time
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

# Ensure project root is in sys.path
PROJECT_ROOT = Path(__file__).resolve().parent.parent
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

from data.kline_cache import KlineCache, TradeRecord, DurationMetricsCalculator
from tests.test_harness import (
    CANONICAL_SYMBOLS,
    StrategyEngine,
    RankingEngine,
    ConfigManager,
    TimeDecayRule,
    TAKER_FEE_RATE,
    SLIPPAGE_RATE,
    MIN_NOTIONAL,
    MAX_RISK_BUDGET_PCT,
)


@dataclass
class StrategyPreset:
    """Configuration preset for multi-strategy benchmarking."""
    name: str
    timeframe: str
    atr_mult_sl: float
    atr_mult_tp: float
    tighten_hours: float
    hard_exit_hours: float
    split_tp_enabled: bool = False
    volatility_scaled: bool = False
    description: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "timeframe": self.timeframe,
            "atr_mult_sl": self.atr_mult_sl,
            "atr_mult_tp": self.atr_mult_tp,
            "tighten_hours": self.tighten_hours,
            "hard_exit_hours": self.hard_exit_hours,
            "split_tp_enabled": self.split_tp_enabled,
            "volatility_scaled": self.volatility_scaled,
            "description": self.description,
        }


class TimeDecayOptimizer:
    """
    Loads 13 cached symbols and benchmarks the 5 Milestone 2 strategies
    with single-slot concurrency and micro-capital risk management.
    """

    def __init__(self, cache_dir: Optional[Path | str] = None):
        self.cache_dir = Path(cache_dir) if cache_dir else PROJECT_ROOT / "data" / "cache"
        self.cache = KlineCache(self.cache_dir)
        self.symbols = list(CANONICAL_SYMBOLS.keys())
        self.klines_1h: Dict[str, List[Tuple]] = {}
        self.klines_15m: Dict[str, List[Tuple]] = {}
        self._load_all_cached_data()

    def _load_all_cached_data(self) -> None:
        """Loads 1h and 15m cached datasets into memory for fast execution."""
        for s in self.symbols:
            raw_1h = self.cache.load_klines(s, "1h", validate=False)
            self.klines_1h[s] = [tuple(bar) for bar in raw_1h]

            raw_15m = self.cache.load_klines(s, "15m", validate=False)
            self.klines_15m[s] = [tuple(bar) for bar in raw_15m]

    def get_presets(self) -> List[StrategyPreset]:
        """Returns the 5 canonical Milestone 2 benchmark strategy presets."""
        return [
            StrategyPreset(
                name="Strategy 1: Tri-Regime Baseline (1h, No Decay)",
                timeframe="1h",
                atr_mult_sl=1.8,
                atr_mult_tp=3.6,
                tighten_hours=500.0,
                hard_exit_hours=1000.0,
                split_tp_enabled=False,
                volatility_scaled=False,
                description="Tri-Regime v8.3 Baseline on 1h candles with SL 1.8 ATR, TP 3.6 ATR, no time-decay exit.",
            ),
            StrategyPreset(
                name="Strategy 2: Tri-Regime + Time-Decay (1h, 12h/24h)",
                timeframe="1h",
                atr_mult_sl=1.8,
                atr_mult_tp=3.6,
                tighten_hours=12.0,
                hard_exit_hours=24.0,
                split_tp_enabled=False,
                volatility_scaled=False,
                description="Tri-Regime v8.3 with 12h Breakeven tightening and 24h Hard Market Exit.",
            ),
            StrategyPreset(
                name="Strategy 3: Tri-Regime + Split-TP (1h, 1.8/3.6 ATR)",
                timeframe="1h",
                atr_mult_sl=1.8,
                atr_mult_tp=3.6,
                tighten_hours=12.0,
                hard_exit_hours=24.0,
                split_tp_enabled=True,
                volatility_scaled=False,
                description="Tri-Regime with Split Take-Profit: 50% closed at 1.8 ATR moving stop to BE, 50% at 3.6 ATR.",
            ),
            StrategyPreset(
                name="Strategy 4: High-Velocity Scalping (15m, 6-bar Decay)",
                timeframe="15m",
                atr_mult_sl=1.5,
                atr_mult_tp=3.0,
                tighten_hours=1.5,  # 6 bars on 15m = 1.5h
                hard_exit_hours=3.0,  # 12 bars on 15m = 3.0h
                split_tp_enabled=False,
                volatility_scaled=False,
                description="High-Velocity 15m Intraday Scalping with 6-bar (1.5h) BE tightening and 12-bar (3h) hard exit.",
            ),
            StrategyPreset(
                name="Strategy 5: Dynamic Volatility-Scaled Time-Decay (1h)",
                timeframe="1h",
                atr_mult_sl=1.8,
                atr_mult_tp=3.6,
                tighten_hours=12.0,
                hard_exit_hours=24.0,
                split_tp_enabled=False,
                volatility_scaled=True,
                description="Dynamic ATR%-scaled time-decay: holding thresholds dynamically clamp between 6h-18h / 12h-36h.",
            ),
        ]

    def evaluate_strategy(
        self,
        preset: StrategyPreset,
        initial_balance: float = 10.0,
    ) -> Dict[str, Any]:
        """
        Executes multi-symbol backtest simulation for a single preset.
        Enforces strict single-slot concurrency and micro-capital risk limits.
        """
        dataset = self.klines_15m if preset.timeframe == "15m" else self.klines_1h
        min_bars = min(len(bars) for bars in dataset.values()) if dataset else 0
        if min_bars < 26:
            return RankingEngine.calculate_strategy_metrics(
                strategy_name=preset.name,
                initial_balance=initial_balance,
                trades=[],
                parameters=preset.to_dict(),
            )

        balance = initial_balance
        closed_trades: List[TradeRecord] = []
        active_position: Optional[Dict[str, Any]] = None

        time_decay_rule = TimeDecayRule(
            tighten_hours=preset.tighten_hours,
            hard_exit_hours=preset.hard_exit_hours,
            fee_offset_pct=0.0025,
        )

        for i in range(25, min_bars):
            # -------------------------------------------------------------
            # 1. Manage Active Position (if open)
            # -------------------------------------------------------------
            if active_position is not None:
                sym = active_position["symbol"]
                bar = dataset[sym][i]
                current_time = bar[0]
                h = bar[2]
                l = bar[3]
                c = bar[4]
                side = active_position["side"]
                entry_p = active_position["entry_price"]
                entry_time = active_position["entry_time"]
                entry_bar = active_position["entry_bar"]
                atr = active_position["atr"]
                tighten_h = active_position["tighten_hours"]
                hard_h = active_position["hard_exit_hours"]
                holding_hours = (current_time - entry_time) / 3600000.0

                # Update peak price
                if side == "LONG":
                    if h > active_position["peak_price"]:
                        active_position["peak_price"] = h
                else:
                    if l < active_position["peak_price"]:
                        active_position["peak_price"] = l
                peak_p = active_position["peak_price"]

                # Branch A: Split-TP Execution
                if active_position.get("is_split_tp", False):
                    t1 = active_position["tranche_1"]
                    t2 = active_position["tranche_2"]

                    # 1. Check Tranche 1 TP (+1.8 ATR)
                    if not t1["closed"]:
                        t1_hit = (side == "LONG" and h >= t1["tp"]) or (side == "SHORT" and l <= t1["tp"])
                        if t1_hit:
                            # Tranche 1 closes at target TP
                            rec_1 = TradeRecord(
                                symbol=sym,
                                side=side,
                                entry_time=entry_time,
                                exit_time=current_time,
                                entry_price=entry_p,
                                exit_price=t1["tp"],
                                qty=t1["qty"],
                                margin_used=t1["margin_used"],
                                exit_reason="TP",
                                entry_bar_idx=entry_bar,
                                exit_bar_idx=i,
                            )
                            balance += rec_1.net_pnl
                            closed_trades.append(rec_1)
                            t1["closed"] = True

                            # Tighten Tranche 2 stop to Breakeven + Commission (+0.25%)
                            be_sl = entry_p * 1.0025 if side == "LONG" else entry_p * 0.9975
                            t2["sl"] = max(t2["sl"], be_sl) if side == "LONG" else min(t2["sl"], be_sl)
                            t2["profit_lock"] = True

                    # 2. Update Tranche 2 Profit Lock (+1.5 ATR) & Trailing Stop (+2.2 ATR)
                    if not t2["profit_lock"]:
                        if side == "LONG" and (peak_p - entry_p) >= (1.5 * atr):
                            t2["profit_lock"] = True
                            t2["sl"] = max(t2["sl"], entry_p * 1.0025)
                        elif side == "SHORT" and (entry_p - peak_p) >= (1.5 * atr):
                            t2["profit_lock"] = True
                            t2["sl"] = min(t2["sl"], entry_p * 0.9975)

                    if side == "LONG" and (peak_p - entry_p) >= (2.2 * atr):
                        trail_sl = peak_p - (1.2 * atr)
                        t2["sl"] = max(t2["sl"], trail_sl)
                    elif side == "SHORT" and (entry_p - peak_p) >= (2.2 * atr):
                        trail_sl = peak_p + (1.2 * atr)
                        t2["sl"] = min(t2["sl"], trail_sl)

                    # 3. Time Decay Evaluation
                    should_time_exit = False
                    if holding_hours >= hard_h:
                        should_time_exit = True
                    elif holding_hours >= tighten_h:
                        peak_gain = (peak_p - entry_p) if side == "LONG" else (entry_p - peak_p)
                        if peak_gain < atr:
                            tightened_sl = entry_p * 1.0025 if side == "LONG" else entry_p * 0.9975
                            if side == "LONG":
                                if not t1["closed"]:
                                    t1["sl"] = max(t1["sl"], tightened_sl)
                                if not t2["profit_lock"]:
                                    t2["sl"] = max(t2["sl"], tightened_sl)
                                if c <= tightened_sl:
                                    should_time_exit = True
                            else:
                                if not t1["closed"]:
                                    t1["sl"] = min(t1["sl"], tightened_sl)
                                if not t2["profit_lock"]:
                                    t2["sl"] = min(t2["sl"], tightened_sl)
                                if c >= tightened_sl:
                                    should_time_exit = True

                    # 4. Exits Evaluation
                    if t1["closed"]:
                        # Only Tranche 2 remains active
                        exit_price = None
                        exit_reason = None
                        if side == "LONG":
                            if h >= t2["tp"]:
                                exit_price = t2["tp"]
                                exit_reason = "TP"
                            elif l <= t2["sl"]:
                                exit_price = t2["sl"]
                                if not t2["profit_lock"] and holding_hours >= tighten_h and t2["sl"] >= entry_p * 1.0025:
                                    exit_reason = "TIME_DECAY"
                                elif t2["profit_lock"] and t2["sl"] >= entry_p:
                                    exit_reason = "PROFIT_LOCK"
                                else:
                                    exit_reason = "SL"
                            elif should_time_exit:
                                exit_price = c
                                exit_reason = "TIME_DECAY"
                        else:  # SHORT
                            if l <= t2["tp"]:
                                exit_price = t2["tp"]
                                exit_reason = "TP"
                            elif h >= t2["sl"]:
                                exit_price = t2["sl"]
                                if not t2["profit_lock"] and holding_hours >= tighten_h and t2["sl"] <= entry_p * 0.9975:
                                    exit_reason = "TIME_DECAY"
                                elif t2["profit_lock"] and t2["sl"] <= entry_p:
                                    exit_reason = "PROFIT_LOCK"
                                else:
                                    exit_reason = "SL"
                            elif should_time_exit:
                                exit_price = c
                                exit_reason = "TIME_DECAY"

                        if exit_price is not None:
                            rec_2 = TradeRecord(
                                symbol=sym,
                                side=side,
                                entry_time=entry_time,
                                exit_time=current_time,
                                entry_price=entry_p,
                                exit_price=exit_price,
                                qty=t2["qty"],
                                margin_used=t2["margin_used"],
                                exit_reason=exit_reason,
                                entry_bar_idx=entry_bar,
                                exit_bar_idx=i,
                            )
                            balance += rec_2.net_pnl
                            closed_trades.append(rec_2)
                            active_position = None  # Liberate single slot

                    else:
                        # Both tranches still active: check SL or hard time-decay
                        exit_price = None
                        exit_reason = None
                        if side == "LONG":
                            if l <= t1["sl"]:
                                exit_price = t1["sl"]
                                exit_reason = "TIME_DECAY" if (holding_hours >= tighten_h and t1["sl"] >= entry_p * 1.0025) else "SL"
                            elif should_time_exit:
                                exit_price = c
                                exit_reason = "TIME_DECAY"
                        else:  # SHORT
                            if h >= t1["sl"]:
                                exit_price = t1["sl"]
                                exit_reason = "TIME_DECAY" if (holding_hours >= tighten_h and t1["sl"] <= entry_p * 0.9975) else "SL"
                            elif should_time_exit:
                                exit_price = c
                                exit_reason = "TIME_DECAY"

                        if exit_price is not None:
                            # Both tranches exit at exit_price
                            rec_1 = TradeRecord(
                                symbol=sym,
                                side=side,
                                entry_time=entry_time,
                                exit_time=current_time,
                                entry_price=entry_p,
                                exit_price=exit_price,
                                qty=t1["qty"],
                                margin_used=t1["margin_used"],
                                exit_reason=exit_reason,
                                entry_bar_idx=entry_bar,
                                exit_bar_idx=i,
                            )
                            rec_2 = TradeRecord(
                                symbol=sym,
                                side=side,
                                entry_time=entry_time,
                                exit_time=current_time,
                                entry_price=entry_p,
                                exit_price=exit_price,
                                qty=t2["qty"],
                                margin_used=t2["margin_used"],
                                exit_reason=exit_reason,
                                entry_bar_idx=entry_bar,
                                exit_bar_idx=i,
                            )
                            balance += rec_1.net_pnl + rec_2.net_pnl
                            closed_trades.extend([rec_1, rec_2])
                            active_position = None  # Liberate single slot

                # Branch B: Standard Single-Tranche Position Execution
                else:
                    sl = active_position["sl"]
                    tp = active_position["tp"]
                    qty = active_position["qty"]
                    margin_used = active_position["margin_used"]
                    profit_lock = active_position["profit_lock"]

                    # Profit lock (+1.5 ATR)
                    if not profit_lock:
                        if side == "LONG" and (peak_p - entry_p) >= (1.5 * atr):
                            profit_lock = True
                            active_position["profit_lock"] = True
                            sl = max(sl, entry_p * 1.0025)
                            active_position["sl"] = sl
                        elif side == "SHORT" and (entry_p - peak_p) >= (1.5 * atr):
                            profit_lock = True
                            active_position["profit_lock"] = True
                            sl = min(sl, entry_p * 0.9975)
                            active_position["sl"] = sl

                    # Trailing stop (+2.2 ATR)
                    if side == "LONG" and (peak_p - entry_p) >= (2.2 * atr):
                        trail_sl = peak_p - (1.2 * atr)
                        sl = max(sl, trail_sl)
                        active_position["sl"] = sl
                    elif side == "SHORT" and (entry_p - peak_p) >= (2.2 * atr):
                        trail_sl = peak_p + (1.2 * atr)
                        sl = min(sl, trail_sl)
                        active_position["sl"] = sl

                    # Time-Decay Evaluation
                    should_time_exit = False
                    time_reason = None
                    if holding_hours >= hard_h:
                        should_time_exit = True
                        time_reason = "TIME_DECAY"
                    elif holding_hours >= tighten_h and not profit_lock:
                        peak_gain = (peak_p - entry_p) if side == "LONG" else (entry_p - peak_p)
                        if peak_gain < atr:
                            if side == "LONG":
                                tightened_sl = entry_p * 1.0025
                                if sl < tightened_sl:
                                    sl = tightened_sl
                                    active_position["sl"] = sl
                                    if c <= tightened_sl:
                                        should_time_exit = True
                                        time_reason = "TIME_DECAY"
                            else:
                                tightened_sl = entry_p * 0.9975
                                if sl > tightened_sl:
                                    sl = tightened_sl
                                    active_position["sl"] = sl
                                    if c >= tightened_sl:
                                        should_time_exit = True
                                        time_reason = "TIME_DECAY"

                    # Check Exits: TP, SL, Trailing, or Time-Decay
                    exit_price = None
                    exit_reason = None

                    if side == "LONG":
                        if h >= tp:
                            exit_price = tp
                            exit_reason = "TP"
                        elif l <= sl:
                            exit_price = sl
                            if not profit_lock and holding_hours >= tighten_h and sl >= entry_p * 1.0025:
                                exit_reason = "TIME_DECAY"
                            elif profit_lock and sl >= entry_p:
                                exit_reason = "PROFIT_LOCK"
                            else:
                                exit_reason = "SL"
                        elif should_time_exit:
                            exit_price = c
                            exit_reason = time_reason
                    else:  # SHORT
                        if l <= tp:
                            exit_price = tp
                            exit_reason = "TP"
                        elif h >= sl:
                            exit_price = sl
                            if not profit_lock and holding_hours >= tighten_h and sl <= entry_p * 0.9975:
                                exit_reason = "TIME_DECAY"
                            elif profit_lock and sl <= entry_p:
                                exit_reason = "PROFIT_LOCK"
                            else:
                                exit_reason = "SL"
                        elif should_time_exit:
                            exit_price = c
                            exit_reason = time_reason

                    if exit_price is not None:
                        trade = TradeRecord(
                            symbol=sym,
                            side=side,
                            entry_time=entry_time,
                            exit_time=current_time,
                            entry_price=entry_p,
                            exit_price=exit_price,
                            qty=qty,
                            margin_used=margin_used,
                            exit_reason=exit_reason,
                            entry_bar_idx=entry_bar,
                            exit_bar_idx=i,
                        )
                        balance += trade.net_pnl
                        closed_trades.append(trade)
                        active_position = None  # Liberate single slot

            # -------------------------------------------------------------
            # 2. Scan for Signal if Flat
            # -------------------------------------------------------------
            if active_position is None and balance >= 5.05:
                best_signal = None
                best_score = 0.0

                for sym in self.symbols:
                    if sym not in dataset:
                        continue
                    bars_up_to_i = dataset[sym][: i + 1]
                    regime, side, score = StrategyEngine.evaluate_regimes(bars_up_to_i)
                    if regime > 0 and score >= 75.0 and score > best_score:
                        atr = StrategyEngine.calculate_atr(bars_up_to_i, 14)
                        if atr > 0:
                            best_signal = (
                                sym,
                                side,
                                score,
                                atr,
                                bars_up_to_i[-1][4],
                                bars_up_to_i[-1][0],
                            )
                            best_score = score

                if best_signal is not None:
                    sym, side, score, atr, entry_p, entry_time = best_signal
                    sizing = StrategyEngine.calculate_order_sizing(
                        balance=balance,
                        entry_price=entry_p,
                        atr=atr,
                        symbol=sym,
                        atr_mult_sl=preset.atr_mult_sl,
                        atr_mult_tp=preset.atr_mult_tp,
                    )
                    if sizing is not None:
                        # Determine decay thresholds
                        if preset.volatility_scaled:
                            atr_pct = (atr / entry_p) * 100.0 if entry_p > 0 else 2.0
                            if atr_pct > 0:
                                t_bars = max(6, min(18, round(12.0 * (2.0 / atr_pct))))
                                h_bars = max(12, min(36, round(24.0 * (2.0 / atr_pct))))
                                t_hours = float(t_bars)
                                h_hours = float(h_bars)
                            else:
                                t_hours = 12.0
                                h_hours = 24.0
                        else:
                            t_hours = preset.tighten_hours
                            h_hours = preset.hard_exit_hours

                        sl_dist = sizing["sl_distance"]
                        tp_dist = sizing["tp_distance"]
                        initial_sl = entry_p - sl_dist if side == "LONG" else entry_p + sl_dist
                        target_tp = entry_p + tp_dist if side == "LONG" else entry_p - tp_dist

                        # Split-TP position construction
                        if preset.split_tp_enabled:
                            spec = CANONICAL_SYMBOLS[sym]
                            step = spec["step_size"]
                            raw_half = sizing["qty"] * 0.5
                            steps_1 = math.floor(raw_half / step)
                            qty1 = round(steps_1 * step, spec["qty_dec"])
                            if qty1 < spec["min_qty"]:
                                qty1 = spec["min_qty"]
                            qty2 = round(sizing["qty"] - qty1, spec["qty_dec"])

                            if qty2 >= spec["min_qty"]:
                                m1 = round(sizing["margin_used"] * (qty1 / sizing["qty"]), 4)
                                m2 = round(sizing["margin_used"] - m1, 4)
                                tp1 = entry_p + (1.8 * atr) if side == "LONG" else entry_p - (1.8 * atr)
                                active_position = {
                                    "symbol": sym,
                                    "side": side,
                                    "entry_price": entry_p,
                                    "entry_time": entry_time,
                                    "entry_bar": i,
                                    "atr": atr,
                                    "peak_price": entry_p,
                                    "tighten_hours": t_hours,
                                    "hard_exit_hours": h_hours,
                                    "is_split_tp": True,
                                    "tranche_1": {
                                        "qty": qty1,
                                        "margin_used": m1,
                                        "sl": initial_sl,
                                        "tp": tp1,
                                        "closed": False,
                                    },
                                    "tranche_2": {
                                        "qty": qty2,
                                        "margin_used": m2,
                                        "sl": initial_sl,
                                        "tp": target_tp,
                                        "closed": False,
                                        "profit_lock": False,
                                    },
                                }
                            else:
                                # Fallback to standard if lot size cannot be split
                                active_position = {
                                    "symbol": sym,
                                    "side": side,
                                    "entry_price": entry_p,
                                    "entry_time": entry_time,
                                    "entry_bar": i,
                                    "qty": sizing["qty"],
                                    "margin_used": sizing["margin_used"],
                                    "sl": initial_sl,
                                    "tp": target_tp,
                                    "peak_price": entry_p,
                                    "atr": atr,
                                    "profit_lock": False,
                                    "tighten_hours": t_hours,
                                    "hard_exit_hours": h_hours,
                                    "is_split_tp": False,
                                }
                        else:
                            active_position = {
                                "symbol": sym,
                                "side": side,
                                "entry_price": entry_p,
                                "entry_time": entry_time,
                                "entry_bar": i,
                                "qty": sizing["qty"],
                                "margin_used": sizing["margin_used"],
                                "sl": initial_sl,
                                "tp": target_tp,
                                "peak_price": entry_p,
                                "atr": atr,
                                "profit_lock": False,
                                "tighten_hours": t_hours,
                                "hard_exit_hours": h_hours,
                                "is_split_tp": False,
                            }

        # End of simulation: calculate metrics
        return RankingEngine.calculate_strategy_metrics(
            strategy_name=preset.name,
            initial_balance=initial_balance,
            trades=closed_trades,
            parameters=preset.to_dict(),
        )

    def run_all_and_rank(self) -> List[Dict[str, Any]]:
        """Runs all 5 benchmark strategies and ranks them descending by TAS score."""
        presets = self.get_presets()
        results = []
        for p in presets:
            t0 = time.perf_counter()
            res = self.evaluate_strategy(p)
            dur_ms = (time.perf_counter() - t0) * 1000
            res["benchmark_latency_ms"] = round(dur_ms, 2)
            results.append(res)

        return RankingEngine.rank_strategies(results, sort_by="tas_score")

    def generate_and_save_report(
        self,
        output_file: Optional[Path | str] = None,
    ) -> Path:
        """
        Runs optimization across all 5 strategies, saves the JSON report to
        reports/milestone2_optimization.json, and prints a comparative summary.
        """
        out_path = Path(output_file) if output_file else PROJECT_ROOT / "reports" / "milestone2_optimization.json"
        out_path.parent.mkdir(parents=True, exist_ok=True)

        t_start = time.perf_counter()
        ranked = self.run_all_and_rank()
        total_time_ms = (time.perf_counter() - t_start) * 1000

        summary_rows = []
        for rank, item in enumerate(ranked, 1):
            m = item["metrics"]
            p = item["parameters"]
            summary_rows.append({
                "rank": rank,
                "strategy_name": item["strategy_name"],
                "timeframe": p.get("timeframe", "1h"),
                "net_profit_pct": m["net_profit_pct"],
                "win_rate_pct": m["win_rate_pct"],
                "profit_factor": m["profit_factor"],
                "max_drawdown_pct": m["max_drawdown_pct"],
                "total_trades": m["total_trades"],
                "avg_duration_hours": m["avg_duration_hours"],
                "active_capital_hourly_roi": m["active_capital_hourly_roi"],
                "tas_score": m["tas_score"],
                "sharpe_ratio": m["sharpe_ratio"],
                "calmar_ratio": m["calmar_ratio"],
            })

        markdown_table = RankingEngine.generate_comparative_markdown(ranked)

        report_payload = {
            "title": "Milestone 2 Time-Weighted Multi-Strategy Optimization Report",
            "generated_at_utc": datetime.now(timezone.utc).isoformat(),
            "total_benchmark_latency_ms": round(total_time_ms, 2),
            "symbols_tested": self.symbols,
            "strategies_count": len(ranked),
            "summary_table": summary_rows,
            "markdown_table": markdown_table,
            "ranked_strategies": ranked,
        }

        with open(out_path, "w", encoding="utf-8") as f:
            json.dump(report_payload, f, indent=2)

        return out_path


def main() -> int:
    print("=" * 80)
    print("🚀 MILESTONE 2: TIME-WEIGHTED MULTI-STRATEGY OPTIMIZATION ENGINE")
    print("Benchmarking 5 Canonical Strategies Across 13 Crypto Pairs (1h & 15m)")
    print("=" * 80)

    t0 = time.perf_counter()
    optimizer = TimeDecayOptimizer()
    report_path = optimizer.generate_and_save_report()
    elapsed = time.perf_counter() - t0

    # Read and print summary
    with open(report_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    print(f"\n✅ Completed optimization across all 5 strategies in {elapsed:.2f}s ({data['total_benchmark_latency_ms']:.1f}ms simulation).")
    print(f"📄 Report written to: {report_path} ({report_path.stat().st_size} bytes)")
    print("\n" + data["markdown_table"] + "\n")

    return 0


if __name__ == "__main__":
    sys.exit(main())
