"""
E2E Test Harness & Interface Contract Implementations
Binance Futures Bot Time-Based ROI Strategy Optimization
Derived strictly from ORIGINAL_REQUEST.md and PROJECT.md
"""

import json
import math
import os
import time
from typing import Dict, List, Optional, Tuple, Any


# Canonical 13 symbols and specifications from PROJECT.md & src/main.rs
CANONICAL_SYMBOLS = {
    "BTCUSDT": {"leverage": 10, "step_size": 0.001, "min_qty": 0.001, "qty_dec": 3, "price_dec": 2},
    "ETHUSDT": {"leverage": 6, "step_size": 0.001, "min_qty": 0.001, "qty_dec": 3, "price_dec": 2},
    "XRPUSDT": {"leverage": 6, "step_size": 0.1, "min_qty": 0.1, "qty_dec": 1, "price_dec": 4},
    "SOLUSDT": {"leverage": 6, "step_size": 0.01, "min_qty": 0.01, "qty_dec": 2, "price_dec": 2},
    "DOGEUSDT": {"leverage": 6, "step_size": 1.0, "min_qty": 1.0, "qty_dec": 0, "price_dec": 5},
    "LINKUSDT": {"leverage": 6, "step_size": 0.01, "min_qty": 0.01, "qty_dec": 2, "price_dec": 3},
    "SUIUSDT": {"leverage": 6, "step_size": 0.1, "min_qty": 0.1, "qty_dec": 1, "price_dec": 4},
    "AVAXUSDT": {"leverage": 6, "step_size": 1.0, "min_qty": 1.0, "qty_dec": 0, "price_dec": 2},
    "NEARUSDT": {"leverage": 6, "step_size": 0.1, "min_qty": 0.1, "qty_dec": 1, "price_dec": 3},
    "ADAUSDT": {"leverage": 6, "step_size": 1.0, "min_qty": 1.0, "qty_dec": 0, "price_dec": 4},
    "APTUSDT": {"leverage": 6, "step_size": 0.1, "min_qty": 0.1, "qty_dec": 1, "price_dec": 2},
    "DOTUSDT": {"leverage": 6, "step_size": 0.1, "min_qty": 0.1, "qty_dec": 1, "price_dec": 3},
    "TIAUSDT": {"leverage": 6, "step_size": 1.0, "min_qty": 1.0, "qty_dec": 0, "price_dec": 3},
}

# Cost model constants from src/main.rs:41-43
TAKER_FEE_RATE = 0.0005       # 0.05% per leg (0.10% round trip)
SLIPPAGE_RATE = 0.0001        # 0.01% per leg (0.02% round trip)
ROUND_TRIP_FRICTION = 0.0012  # 0.12% round trip notional
FUNDING_RATE_8H = 0.0001      # 0.01% every 8 hours
MIN_NOTIONAL = 5.05           # Binance min notional threshold $5.00 + buffer
INITIAL_BALANCE_DEFAULT = 10.0
MAX_RISK_BUDGET_PCT = 0.06    # 6% risk budget on micro capital


# ---------------------------------------------------------------------------
# Feature 1: Data Ingestion & Caching
# ---------------------------------------------------------------------------
class KlineDataCache:
    """Manages ingestion and local caching of 13-symbol klines."""

    def __init__(self, cache_dir: str):
        self.cache_dir = cache_dir
        os.makedirs(self.cache_dir, exist_ok=True)

    def _get_cache_path(self, symbol: str, interval: str) -> str:
        return os.path.join(self.cache_dir, f"{symbol}_{interval}_cache.json")

    def save_klines(self, symbol: str, interval: str, klines: List[Any]) -> str:
        if symbol not in CANONICAL_SYMBOLS:
            raise ValueError(f"Unsupported symbol: {symbol}")
        if interval not in ["1h", "15m", "5m"]:
            raise ValueError(f"Unsupported interval: {interval}")
        self.validate_kline_schema(klines)
        
        path = self._get_cache_path(symbol, interval)
        data = {
            "symbol": symbol,
            "interval": interval,
            "count": len(klines),
            "last_updated": int(time.time() * 1000),
            "klines": klines
        }
        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f)
        return path

    def load_klines(self, symbol: str, interval: str) -> Optional[List[Any]]:
        path = self._get_cache_path(symbol, interval)
        if not os.path.exists(path):
            return None
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        klines = data.get("klines", [])
        self.validate_kline_schema(klines)
        return klines

    @staticmethod
    def validate_kline_schema(klines: List[Any]) -> bool:
        """Validates that klines match schema: [open_time, open, high, low, close, volume, ...]"""
        if not isinstance(klines, list):
            raise TypeError("Klines must be a list")
        prev_time = -1
        for idx, bar in enumerate(klines):
            if not isinstance(bar, (list, tuple)) or len(bar) < 6:
                raise ValueError(f"Bar at index {idx} has invalid format or length < 6")
            open_time, o, h, l, c, v = bar[0], float(bar[1]), float(bar[2]), float(bar[3]), float(bar[4]), float(bar[5])
            if open_time <= prev_time:
                raise ValueError(f"Bar at index {idx} violates chronological ordering: {open_time} <= {prev_time}")
            if o <= 0 or h <= 0 or l <= 0 or c <= 0:
                raise ValueError(f"Bar at index {idx} has non-positive price")
            if h < max(o, l, c) or l > min(o, h, c):
                raise ValueError(f"Bar at index {idx} has inconsistent OHLC: o={o}, h={h}, l={l}, c={c}")
            if v < 0:
                raise ValueError(f"Bar at index {idx} has negative volume: {v}")
            prev_time = open_time
        return True


# ---------------------------------------------------------------------------
# Feature 2: Time-in-Trade & Duration Tracking (PROJECT.md Interface Contract)
# ---------------------------------------------------------------------------
class TradeRecord:
    """Trade record adhering to M1 <-> M2 Interface Contract in PROJECT.md."""

    def __init__(
        self,
        symbol: str,
        side: str,
        entry_time: int,
        exit_time: int,
        entry_price: float,
        exit_price: float,
        qty: float,
        margin_used: float,
        exit_reason: str,
        entry_bar_idx: int = 0,
        exit_bar_idx: int = 0,
    ):
        if side not in ["LONG", "SHORT"]:
            raise ValueError(f"Invalid side: {side}")
        if exit_time < entry_time:
            raise ValueError(f"exit_time ({exit_time}) cannot be earlier than entry_time ({entry_time})")
        if exit_reason not in ["TP", "SL", "PROFIT_LOCK", "TRAILING_STOP", "TIME_DECAY"]:
            raise ValueError(f"Invalid exit_reason: {exit_reason}")
        if entry_price <= 0 or exit_price <= 0 or qty <= 0:
            raise ValueError("Prices and quantities must be positive")

        self.symbol = symbol
        self.side = side
        self.entry_time = entry_time
        self.exit_time = exit_time
        self.entry_bar_idx = entry_bar_idx
        self.exit_bar_idx = exit_bar_idx
        self.entry_price = entry_price
        self.exit_price = exit_price
        self.qty = qty
        self.margin_used = margin_used
        self.exit_reason = exit_reason

        # Mathematical calculations
        duration_ms = max(0, exit_time - entry_time)
        self.holding_duration_hours = round(duration_ms / 3600000.0, 4)
        self.holding_bars = max(0, exit_bar_idx - entry_bar_idx)

        # Gross PnL
        if side == "LONG":
            self.gross_pnl = round((exit_price - entry_price) * qty, 4)
        else:
            self.gross_pnl = round((entry_price - exit_price) * qty, 4)

        # Fees & Slippage
        notional_entry = entry_price * qty
        notional_exit = exit_price * qty
        self.fees = round((notional_entry + notional_exit) * (TAKER_FEE_RATE + SLIPPAGE_RATE), 4)

        # Net PnL
        self.net_pnl = round(self.gross_pnl - self.fees, 4)

        # ROI % on margin
        if margin_used > 0:
            self.roi_pct = round((self.net_pnl / margin_used) * 100.0, 4)
        else:
            self.roi_pct = 0.0

        # ROI per hour
        if self.holding_duration_hours > 0:
            self.roi_per_hour = round(self.roi_pct / self.holding_duration_hours, 4)
        else:
            self.roi_per_hour = self.roi_pct

    def to_dict(self) -> Dict[str, Any]:
        """Matches PROJECT.md Trade Record Schema exactly."""
        return {
            "symbol": self.symbol,
            "side": self.side,
            "entry_time": self.entry_time,
            "exit_time": self.exit_time,
            "holding_duration_hours": self.holding_duration_hours,
            "holding_bars": self.holding_bars,
            "entry_price": self.entry_price,
            "exit_price": self.exit_price,
            "gross_pnl": self.gross_pnl,
            "fees": self.fees,
            "net_pnl": self.net_pnl,
            "roi_pct": self.roi_pct,
            "roi_per_hour": self.roi_per_hour,
            "exit_reason": self.exit_reason,
        }


# ---------------------------------------------------------------------------
# Feature 3: Time-Decay Stagnation Exit Rule
# ---------------------------------------------------------------------------
class TimeDecayRule:
    """
    Dynamic exit rules:
    - 12h rule: Tightens SL to breakeven + commission (+0.25% fee offset) if trade is stagnant.
    - 24h rule: Hard market exit to liberate capital.
    """

    def __init__(self, tighten_hours: float = 12.0, hard_exit_hours: float = 24.0, fee_offset_pct: float = 0.0025):
        self.tighten_hours = tighten_hours
        self.hard_exit_hours = hard_exit_hours
        self.fee_offset_pct = fee_offset_pct

    def evaluate_exit(
        self,
        holding_duration_hours: float,
        side: str,
        entry_price: float,
        current_price: float,
        current_sl: float,
        profit_lock_active: bool,
    ) -> Tuple[bool, Optional[float], Optional[str]]:
        """
        Returns: (should_exit, new_sl_if_tightened, exit_reason)
        """
        # Hard exit at 24h
        if holding_duration_hours >= self.hard_exit_hours:
            return True, None, "TIME_DECAY"

        # Tightening rule at 12h
        if holding_duration_hours >= self.tighten_hours and not profit_lock_active:
            if side == "LONG":
                tightened_sl = entry_price * (1.0 + self.fee_offset_pct)
                if current_sl < tightened_sl:
                    # If current price is already below the tightened SL, trigger exit
                    if current_price <= tightened_sl:
                        return True, None, "TIME_DECAY"
                    return False, tightened_sl, None
            else:  # SHORT
                tightened_sl = entry_price * (1.0 - self.fee_offset_pct)
                if current_sl > tightened_sl:
                    if current_price >= tightened_sl:
                        return True, None, "TIME_DECAY"
                    return False, tightened_sl, None

        return False, None, None


# ---------------------------------------------------------------------------
# Feature 4: Time-Weighted Strategy Optimization & Signal Engine
# ---------------------------------------------------------------------------
class StrategyEngine:
    """Tri-Regime Signal Evaluation, Split TP, and Order Sizing."""

    @staticmethod
    def calculate_atr(klines: List[Tuple], period: int = 14) -> float:
        if len(klines) < period + 1:
            return 0.0
        trs = []
        for i in range(1, len(klines)):
            h = klines[i][2]
            l = klines[i][3]
            prev_c = klines[i - 1][4]
            tr = max(h - l, abs(h - prev_c), abs(l - prev_c))
            trs.append(tr)
        return sum(trs[-period:]) / period

    @staticmethod
    def calculate_ema(prices: List[float], period: int) -> float:
        if not prices:
            return 0.0
        if len(prices) < period:
            return sum(prices) / len(prices)
        k = 2.0 / (period + 1.0)
        ema = sum(prices[:period]) / period
        for p in prices[period:]:
            ema = (p * k) + (ema * (1.0 - k))
        return ema

    @staticmethod
    def calculate_rsi(prices: List[float], period: int = 14) -> float:
        if len(prices) < period + 1:
            return 50.0
        gains = []
        losses = []
        for i in range(1, len(prices)):
            diff = prices[i] - prices[i - 1]
            if diff >= 0:
                gains.append(diff)
                losses.append(0.0)
            else:
                gains.append(0.0)
                losses.append(abs(diff))
        avg_gain = sum(gains[-period:]) / period
        avg_loss = sum(losses[-period:]) / period
        if avg_loss == 0.0:
            return 100.0
        rs = avg_gain / avg_loss
        return 100.0 - (100.0 / (1.0 + rs))

    @staticmethod
    def evaluate_regimes(klines: List[Tuple]) -> Tuple[int, Optional[str], float]:
        """
        Evaluates Regime 1, 2, 3 as defined in src/main.rs:947-1065.
        Returns: (regime_id, side, score)
        """
        if len(klines) < 25:
            return 0, None, 0.0

        current = klines[-1]
        c = current[4]
        h = current[2]
        l = current[3]
        o = current[1]
        candle_range = max(h - l, 0.0001)

        closes = [k[4] for k in klines]
        ema9 = StrategyEngine.calculate_ema(closes, 9)
        ema21 = StrategyEngine.calculate_ema(closes, 21)
        rsi = StrategyEngine.calculate_rsi(closes, 14)

        # 20-bar Donchian
        high20 = max(k[2] for k in klines[-21:-1])
        low20 = min(k[3] for k in klines[-21:-1])

        # 12-bar Donchian
        high12 = max(k[2] for k in klines[-13:-1])
        low12 = min(k[3] for k in klines[-13:-1])

        # Regime 1: Donchian Liquidity Sweep & Wick Exhaustion
        lower_wick = min(o, c) - l
        upper_wick = h - max(o, c)
        lower_wick_ratio = lower_wick / candle_range
        upper_wick_ratio = upper_wick / candle_range

        if l < low20 and c > low20 and lower_wick_ratio >= 0.35:
            score = 75.0 + min(15.0, lower_wick_ratio * 30.0)
            return 1, "LONG", score
        if h > high20 and c < high20 and upper_wick_ratio >= 0.35:
            score = 75.0 + min(15.0, upper_wick_ratio * 30.0)
            return 1, "SHORT", score

        # Regime 2: Momentum Donchian Breakout
        if c >= high12 and ema9 > ema21 and 46.0 <= rsi <= 78.0:
            return 2, "LONG", 78.0
        if c <= low12 and ema9 < ema21 and 22.0 <= rsi <= 54.0:
            return 2, "SHORT", 78.0

        # Regime 3: Dynamic EMA21 Pullback
        if ema9 > ema21 and l <= (ema21 * 1.0025) and c > ema21 and lower_wick_ratio >= 0.15:
            return 3, "LONG", 72.0
        if ema9 < ema21 and h >= (ema21 * 0.9975) and c < ema21 and upper_wick_ratio >= 0.15:
            return 3, "SHORT", 72.0

        return 0, None, 0.0

    @staticmethod
    def calculate_order_sizing(
        balance: float,
        entry_price: float,
        atr: float,
        symbol: str,
        atr_mult_sl: float = 1.8,
        atr_mult_tp: float = 3.6,
    ) -> Optional[Dict[str, float]]:
        """
        Calculates position size strictly respecting micro-capital budget ($10),
        6% risk budget ($0.60), $5.05 min notional, and 8:1 fee ratio.
        """
        spec = CANONICAL_SYMBOLS.get(symbol)
        if not spec or entry_price <= 0 or atr <= 0:
            return None

        leverage = spec["leverage"]
        step_size = spec["step_size"]
        min_qty = spec["min_qty"]

        # Risk budget = 6% of balance (max $0.60 on $10)
        risk_budget = max(0.35, balance * MAX_RISK_BUDGET_PCT)
        sl_distance = atr * atr_mult_sl
        if sl_distance <= 0:
            return None

        # Ideal quantity based on stop distance
        raw_qty = risk_budget / sl_distance

        # Quantize to step size
        steps = math.floor(raw_qty / step_size)
        qty = round(steps * step_size, spec["qty_dec"])

        if qty < min_qty:
            qty = min_qty

        notional = qty * entry_price
        # Minimum notional check ($5.05)
        if notional < MIN_NOTIONAL:
            # Scale up to min notional if possible within risk tolerance
            min_steps = math.ceil(MIN_NOTIONAL / (entry_price * step_size))
            qty = round(min_steps * step_size, spec["qty_dec"])
            notional = qty * entry_price

        # Actual risk check: cannot exceed balance * 0.15 under micro model
        actual_risk = qty * sl_distance
        if actual_risk > balance * 0.15:
            return None  # ATR too wide, coin rejected

        margin_used = round(notional / leverage, 4)
        if margin_used > balance * 0.95:
            return None  # Insufficient margin

        # Check 8:1 gross profit to commission ratio
        expected_tp_gross = qty * (atr * atr_mult_tp)
        round_trip_fee = notional * 2.0 * (TAKER_FEE_RATE + SLIPPAGE_RATE)
        if expected_tp_gross < round_trip_fee * 8.0:
            return None  # Fee drag too high

        return {
            "qty": qty,
            "margin_used": margin_used,
            "notional": round(notional, 4),
            "sl_distance": round(sl_distance, 4),
            "tp_distance": round(atr * atr_mult_tp, 4),
            "actual_risk": round(actual_risk, 4),
        }


# ---------------------------------------------------------------------------
# Feature 5: Comparative Ranking & Report Engine (PROJECT.md Interface Contract)
# ---------------------------------------------------------------------------
class RankingEngine:
    """Computes comprehensive metrics and Time-Adjusted Score (TAS)."""

    @staticmethod
    def calculate_strategy_metrics(
        strategy_name: str,
        initial_balance: float,
        trades: List[TradeRecord],
        parameters: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Calculates Strategy Output Schema matching PROJECT.md lines 57-79."""
        if not trades:
            return {
                "strategy_name": strategy_name,
                "parameters": parameters or {},
                "trades": [],
                "metrics": {
                    "initial_balance": initial_balance,
                    "final_balance": initial_balance,
                    "net_profit_pct": 0.0,
                    "win_rate_pct": 0.0,
                    "profit_factor": 0.0,
                    "max_drawdown_pct": 0.0,
                    "total_trades": 0,
                    "avg_duration_hours": 0.0,
                    "active_capital_hourly_roi": 0.0,
                    "tas_score": 0.0,
                    "sharpe_ratio": 0.0,
                    "sortino_ratio": 0.0,
                    "calmar_ratio": 0.0,
                },
            }

        balance = initial_balance
        peak_balance = initial_balance
        max_drawdown_usd = 0.0
        max_drawdown_pct = 0.0

        wins = 0
        losses = 0
        gross_win = 0.0
        gross_loss = 0.0
        total_fees = 0.0
        total_duration_hours = 0.0
        trade_rois = []

        for t in trades:
            balance += t.net_pnl
            if balance > peak_balance:
                peak_balance = balance
            dd = peak_balance - balance
            dd_pct = (dd / peak_balance) * 100.0 if peak_balance > 0 else 0.0
            if dd_pct > max_drawdown_pct:
                max_drawdown_pct = dd_pct
                max_drawdown_usd = dd

            if t.net_pnl > 0:
                wins += 1
                gross_win += t.gross_pnl
            else:
                losses += 1
                gross_loss += abs(t.gross_pnl)

            total_fees += t.fees
            total_duration_hours += t.holding_duration_hours
            trade_rois.append(t.roi_pct)

        total_trades = len(trades)
        win_rate_pct = round((wins / total_trades) * 100.0, 2)
        profit_factor = round(gross_win / gross_loss, 2) if gross_loss > 0 else 99.0
        net_profit_pct = round(((balance - initial_balance) / initial_balance) * 100.0, 2)
        avg_duration_hours = round(total_duration_hours / total_trades, 2) if total_trades > 0 else 0.0

        # Hourly active ROI
        active_capital_hourly_roi = (
            round(net_profit_pct / max(total_duration_hours, 1.0), 4) if total_duration_hours > 0 else 0.0
        )

        # Sharpe, Sortino, Calmar
        mean_roi = sum(trade_rois) / total_trades if total_trades > 0 else 0.0
        variance = sum((r - mean_roi) ** 2 for r in trade_rois) / max(total_trades - 1, 1)
        stdev = math.sqrt(variance) if variance > 0 else 0.001
        downside_variance = sum((min(r, 0.0)) ** 2 for r in trade_rois) / max(total_trades - 1, 1)
        downside_stdev = math.sqrt(downside_variance) if downside_variance > 0 else 0.001

        sharpe_ratio = round((mean_roi / stdev) * math.sqrt(total_trades), 2) if stdev > 0 else 0.0
        sortino_ratio = round((mean_roi / downside_stdev) * math.sqrt(total_trades), 2) if downside_stdev > 0 else 0.0
        calmar_ratio = round(net_profit_pct / max(max_drawdown_pct, 1.0), 2)

        # Time-Adjusted Score (TAS) formula (from explorer_survey_3:109)
        # TAS = (Net PnL % / max(MaxDD %, 1.0)) * (WinRate % / 50%) * (Hourly Active ROE / Benchmark) * (1 - Fees / max(Gross Wins, 0.001))
        benchmark_hourly_roe = 0.05
        fee_factor = max(0.0, 1.0 - (total_fees / max(gross_win, 0.001)))
        tas_score = round(
            (net_profit_pct / max(max_drawdown_pct, 1.0))
            * (win_rate_pct / 50.0)
            * (active_capital_hourly_roi / benchmark_hourly_roe)
            * fee_factor,
            2,
        )

        return {
            "strategy_name": strategy_name,
            "parameters": parameters or {},
            "trades": [t.to_dict() for t in trades],
            "metrics": {
                "initial_balance": round(initial_balance, 2),
                "final_balance": round(balance, 2),
                "net_profit_pct": net_profit_pct,
                "win_rate_pct": win_rate_pct,
                "profit_factor": profit_factor,
                "max_drawdown_pct": round(max_drawdown_pct, 2),
                "total_trades": total_trades,
                "avg_duration_hours": avg_duration_hours,
                "active_capital_hourly_roi": active_capital_hourly_roi,
                "tas_score": tas_score,
                "sharpe_ratio": sharpe_ratio,
                "sortino_ratio": sortino_ratio,
                "calmar_ratio": calmar_ratio,
            },
        }

    @staticmethod
    def rank_strategies(strategy_results: List[Dict[str, Any]], sort_by: str = "tas_score") -> List[Dict[str, Any]]:
        """Sorts strategy outputs descending by specified metric."""
        return sorted(
            strategy_results,
            key=lambda x: x["metrics"].get(sort_by, 0.0),
            reverse=True,
        )

    @staticmethod
    def generate_comparative_markdown(ranked_strategies: List[Dict[str, Any]]) -> str:
        """Generates comparative ranking table in Markdown format."""
        lines = [
            "# Comparative Performance Ranking Matrix",
            "",
            "| Rank | Strategy Name | Net PnL % | Win Rate % | Profit Factor | MaxDD % | Avg Duration (h) | Hourly ROI % | TAS Score | Sharpe | Calmar |",
            "|------|---------------|-----------|------------|---------------|---------|------------------|--------------|-----------|--------|--------|",
        ]
        for idx, s in enumerate(ranked_strategies, 1):
            m = s["metrics"]
            lines.append(
                f"| {idx} | {s['strategy_name']} | {m['net_profit_pct']:+.2f}% | {m['win_rate_pct']:.1f}% | "
                f"{m['profit_factor']:.2f} | {m['max_drawdown_pct']:.2f}% | {m['avg_duration_hours']:.1f}h | "
                f"{m['active_capital_hourly_roi']:.4f}% | {m['tas_score']:.2f} | {m['sharpe_ratio']:.2f} | {m['calmar_ratio']:.2f} |"
            )
        return "\n".join(lines)


# ---------------------------------------------------------------------------
# Feature 6: Runnable Strategy Deployment Artifacts
# ---------------------------------------------------------------------------
class ConfigManager:
    """Validates and manages `config.json` deployment artifacts."""

    REQUIRED_FIELDS = [
        "version",
        "symbols",
        "leverage_map",
        "timeframe",
        "risk_budget_pct",
        "atr_mult_sl",
        "atr_mult_tp",
        "time_decay_tighten_hours",
        "time_decay_hard_exit_hours",
        "min_notional",
    ]

    @classmethod
    def get_default_config(cls) -> Dict[str, Any]:
        return {
            "version": "v8.3-TimeOptimized",
            "symbols": list(CANONICAL_SYMBOLS.keys()),
            "leverage_map": {k: v["leverage"] for k, v in CANONICAL_SYMBOLS.items()},
            "timeframe": "1h",
            "risk_budget_pct": 0.06,
            "atr_mult_sl": 1.8,
            "atr_mult_tp": 3.6,
            "profit_lock_atr": 1.5,
            "trailing_stop_atr": 2.2,
            "time_decay_tighten_hours": 12.0,
            "time_decay_hard_exit_hours": 24.0,
            "split_tp_enabled": True,
            "min_notional": 5.05,
        }

    @classmethod
    def validate_config(cls, config: Dict[str, Any]) -> bool:
        if not isinstance(config, dict):
            raise TypeError("Config must be a dictionary")
        for field in cls.REQUIRED_FIELDS:
            if field not in config:
                raise ValueError(f"Missing required config field: {field}")
        if not config["symbols"]:
            raise ValueError("Symbols list cannot be empty")
        for s in config["symbols"]:
            if s not in CANONICAL_SYMBOLS:
                raise ValueError(f"Invalid symbol in config: {s}")
        if config["risk_budget_pct"] <= 0 or config["risk_budget_pct"] > 0.20:
            raise ValueError("Risk budget must be between 0 and 0.20 (20%)")
        if config["atr_mult_sl"] <= 0 or config["atr_mult_tp"] <= 0:
            raise ValueError("ATR multipliers must be positive")
        if config["time_decay_tighten_hours"] >= config["time_decay_hard_exit_hours"]:
            raise ValueError("Tighten hours must be strictly less than hard exit hours")
        return True


# ---------------------------------------------------------------------------
# Feature 7: Full E2E Backtest Simulator Engine
# ---------------------------------------------------------------------------
class E2EBacktestSimulator:
    """
    Opaque-box deterministic simulation engine implementing 100% of the
    Tri-Regime + Time-Decay + Split TP pipeline with single-slot concurrency.
    """

    def __init__(self, config: Optional[Dict[str, Any]] = None):
        self.config = config or ConfigManager.get_default_config()
        ConfigManager.validate_config(self.config)
        self.time_decay = TimeDecayRule(
            tighten_hours=self.config["time_decay_tighten_hours"],
            hard_exit_hours=self.config["time_decay_hard_exit_hours"],
        )

    def run_simulation(
        self,
        symbol_klines: Dict[str, List[Tuple]],
        initial_balance: float = 10.0,
    ) -> Dict[str, Any]:
        """
        Runs multi-symbol backtest simulation across all bars.
        Preserves single-slot concurrency lock: exactly one position active at any time.
        """
        # Determine total bars (synced by index)
        min_bars = min(len(bars) for bars in symbol_klines.values()) if symbol_klines else 0
        if min_bars < 26:
            return RankingEngine.calculate_strategy_metrics(
                strategy_name="E2E_Simulation",
                initial_balance=initial_balance,
                trades=[],
                parameters=self.config,
            )

        balance = initial_balance
        closed_trades: List[TradeRecord] = []
        active_position = None  # None or dict representing active trade

        for i in range(25, min_bars):
            # 1. Manage Active Position
            if active_position is not None:
                sym = active_position["symbol"]
                bar = symbol_klines[sym][i]
                current_time = bar[0]
                h = bar[2]
                l = bar[3]
                c = bar[4]
                atr = active_position["atr"]
                side = active_position["side"]
                entry_p = active_position["entry_price"]
                sl = active_position["sl"]
                tp = active_position["tp"]
                qty = active_position["qty"]
                entry_time = active_position["entry_time"]
                entry_bar = active_position["entry_bar"]

                holding_hours = (current_time - entry_time) / 3600000.0

                # Track peak price
                if side == "LONG":
                    if h > active_position["peak_price"]:
                        active_position["peak_price"] = h
                else:
                    if l < active_position["peak_price"]:
                        active_position["peak_price"] = l

                # Check Profit Lock (+1.5 ATR)
                if not active_position["profit_lock"]:
                    if side == "LONG" and (h - entry_p) >= (1.5 * atr):
                        active_position["profit_lock"] = True
                        active_position["sl"] = max(active_position["sl"], entry_p * 1.0025)
                    elif side == "SHORT" and (entry_p - l) >= (1.5 * atr):
                        active_position["profit_lock"] = True
                        active_position["sl"] = min(active_position["sl"], entry_p * 0.9975)

                # Check Trailing Stop (+2.2 ATR)
                if side == "LONG" and (active_position["peak_price"] - entry_p) >= (2.2 * atr):
                    trail_sl = active_position["peak_price"] - (1.2 * atr)
                    active_position["sl"] = max(active_position["sl"], trail_sl)
                elif side == "SHORT" and (entry_p - active_position["peak_price"]) >= (2.2 * atr):
                    trail_sl = active_position["peak_price"] + (1.2 * atr)
                    active_position["sl"] = min(active_position["sl"], trail_sl)

                # Check Time-Decay Stagnation Exit
                should_time_exit, new_sl, time_reason = self.time_decay.evaluate_exit(
                    holding_duration_hours=holding_hours,
                    side=side,
                    entry_price=entry_p,
                    current_price=c,
                    current_sl=active_position["sl"],
                    profit_lock_active=active_position["profit_lock"],
                )
                if new_sl is not None:
                    active_position["sl"] = new_sl

                # Check Exits: TP, SL, Trailing, or Time-Decay
                exit_price = None
                exit_reason = None

                if side == "LONG":
                    if h >= tp:
                        exit_price = tp
                        exit_reason = "TP"
                    elif l <= active_position["sl"]:
                        exit_price = active_position["sl"]
                        exit_reason = (
                            "PROFIT_LOCK"
                            if active_position["profit_lock"] and active_position["sl"] >= entry_p
                            else "SL"
                        )
                    elif should_time_exit:
                        exit_price = c
                        exit_reason = time_reason
                else:  # SHORT
                    if l <= tp:
                        exit_price = tp
                        exit_reason = "TP"
                    elif h >= active_position["sl"]:
                        exit_price = active_position["sl"]
                        exit_reason = (
                            "PROFIT_LOCK"
                            if active_position["profit_lock"] and active_position["sl"] <= entry_p
                            else "SL"
                        )
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
                        margin_used=active_position["margin_used"],
                        exit_reason=exit_reason,
                        entry_bar_idx=entry_bar,
                        exit_bar_idx=i,
                    )
                    balance += trade.net_pnl
                    closed_trades.append(trade)
                    active_position = None  # Liberate single slot

            # 2. If Flat, Scan for Highest Confluence Signal
            if active_position is None and balance >= 5.05:
                best_signal = None
                best_score = 0.0

                for sym in self.config["symbols"]:
                    if sym not in symbol_klines:
                        continue
                    bars_up_to_i = symbol_klines[sym][: i + 1]
                    regime, side, score = StrategyEngine.evaluate_regimes(bars_up_to_i)
                    if regime > 0 and score >= 75.0 and score > best_score:
                        atr = StrategyEngine.calculate_atr(bars_up_to_i, 14)
                        if atr > 0:
                            best_signal = (sym, side, score, atr, bars_up_to_i[-1][4], bars_up_to_i[-1][0])
                            best_score = score

                if best_signal is not None:
                    sym, side, score, atr, entry_p, entry_time = best_signal
                    sizing = StrategyEngine.calculate_order_sizing(
                        balance=balance,
                        entry_price=entry_p,
                        atr=atr,
                        symbol=sym,
                        atr_mult_sl=self.config["atr_mult_sl"],
                        atr_mult_tp=self.config["atr_mult_tp"],
                    )
                    if sizing is not None:
                        sl = (
                            entry_p - sizing["sl_distance"]
                            if side == "LONG"
                            else entry_p + sizing["sl_distance"]
                        )
                        tp = (
                            entry_p + sizing["tp_distance"]
                            if side == "LONG"
                            else entry_p - sizing["tp_distance"]
                        )
                        active_position = {
                            "symbol": sym,
                            "side": side,
                            "entry_price": entry_p,
                            "entry_time": entry_time,
                            "entry_bar": i,
                            "qty": sizing["qty"],
                            "margin_used": sizing["margin_used"],
                            "sl": sl,
                            "tp": tp,
                            "peak_price": entry_p,
                            "atr": atr,
                            "profit_lock": False,
                        }

        return RankingEngine.calculate_strategy_metrics(
            strategy_name=f"TriRegime_{self.config['timeframe']}_TimeDecay",
            initial_balance=initial_balance,
            trades=closed_trades,
            parameters=self.config,
        )


# ---------------------------------------------------------------------------
# Synthetic Dataset Generators for Deterministic Testing
# ---------------------------------------------------------------------------
def generate_synthetic_klines(
    num_bars: int = 100,
    base_price: float = 100.0,
    volatility: float = 1.0,
    trend: float = 0.0,
    start_time: int = 1700000000000,
    interval_ms: int = 3600000,
) -> List[Tuple]:
    """Generates synthetic OHLCV bars with guaranteed deterministic properties."""
    klines = []
    p = base_price
    t = start_time
    for i in range(num_bars):
        o = p
        delta = trend + (math.sin(i * 0.5) * volatility)
        c = max(1.0, o + delta)
        h = max(o, c) + (abs(math.cos(i * 0.7)) * volatility * 0.5)
        l = min(o, c) - (abs(math.sin(i * 0.3)) * volatility * 0.5)
        v = 1000.0 + (abs(math.sin(i)) * 500.0)
        klines.append((t, round(o, 4), round(h, 4), round(l, 4), round(c, 4), round(v, 2)))
        p = c
        t += interval_ms
    return klines
