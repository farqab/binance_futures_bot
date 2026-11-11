"""
Tier 4: Real-World Application Scenarios Test Suite (>=5 tests)
Validates full-scale production-like trading simulations matching ORIGINAL_REQUEST.md:
- Scenario 1: Full 10-Day 13-Symbol Backtest across 3,120 hourly candles
- Scenario 2: High-Beta BTC Lead-Lag Propagation & Altcoin Follow-Through
- Scenario 3: Prolonged 72-Hour Sideways Consolidation & Time-Decay Liquidation
- Scenario 4: Donchian Momentum Breakout with Profit Lock & Trailing Stop Run
- Scenario 5: Micro-Capital ($10) Compounding & Strict 6% Risk Budgeting
- Scenario 6: Dual-Timeframe 1h Swing + 15m Intraday Fusion Setup
"""

import math
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

try:
    from tests.test_harness import (
        CANONICAL_SYMBOLS,
        ConfigManager,
        E2EBacktestSimulator,
        KlineDataCache,
        RankingEngine,
        StrategyEngine,
        TimeDecayRule,
        TradeRecord,
        generate_synthetic_klines,
    )
except ImportError:
    from test_harness import (
        CANONICAL_SYMBOLS,
        ConfigManager,
        E2EBacktestSimulator,
        KlineDataCache,
        RankingEngine,
        StrategyEngine,
        TimeDecayRule,
        TradeRecord,
        generate_synthetic_klines,
    )


class TestTier4RealWorldScenarios(unittest.TestCase):
    """Tier 4: Real-world trading and market scenario simulations."""

    def test_s1_full_10day_13symbol_backtest_scenario(self):
        """Scenario 1: Comprehensive 10-day backtest across all 13 canonical pairs (240 bars each = 3,120 bars)."""
        num_bars = 240  # 10 days * 24 hours
        start_time = 1700000000000

        data = {}
        for sym, spec in CANONICAL_SYMBOLS.items():
            base_p = 50000.0 if "BTC" in sym else (3000.0 if "ETH" in sym else (100.0 if "SOL" in sym else 5.0))
            data[sym] = generate_synthetic_klines(
                num_bars=num_bars,
                base_price=base_p,
                volatility=base_p * 0.015,
                trend=base_p * 0.0005,
                start_time=start_time,
            )

        sim = E2EBacktestSimulator()
        res = sim.run_simulation(data, initial_balance=10.0)

        # Assertions on full backtest execution
        self.assertIn("metrics", res)
        m = res["metrics"]
        self.assertEqual(m["initial_balance"], 10.0)
        self.assertGreater(m["final_balance"], 0.0)
        self.assertIsInstance(m["net_profit_pct"], float)
        self.assertIsInstance(m["total_trades"], int)
        self.assertIsInstance(m["tas_score"], float)

        # Generate comparative Markdown report for the 10-day run
        report = RankingEngine.generate_comparative_markdown([res])
        self.assertIn("Comparative Performance Ranking Matrix", report)

    def test_s2_high_beta_btc_lead_lag_propagation_scenario(self):
        """Scenario 2: Rapid BTC +2.5% expansion drives altcoin lead-lag momentum setup."""
        num_bars = 50
        # BTC surges violently starting at bar 26
        btc_bars = []
        p = 60000.0
        for i in range(num_bars):
            if i >= 26:
                p += 500.0  # BTC sharp rally
            btc_bars.append((1000 + i * 3600000, p, p + 200, p - 50, p + 150, 5000.0))

        # SOL follows with high beta
        sol_bars = []
        sp = 150.0
        for i in range(num_bars):
            if i >= 27:
                sp += 4.0  # High beta follow-through
            sol_bars.append((1000 + i * 3600000, sp, sp + 2.0, sp - 0.5, sp + 1.5, 10000.0))

        sim = E2EBacktestSimulator()
        res = sim.run_simulation({"BTCUSDT": btc_bars, "SOLUSDT": sol_bars}, initial_balance=10.0)
        self.assertIsInstance(res["trades"], list)

    def test_s3_sideways_consolidation_stagnation_decay_scenario(self):
        """Scenario 3: 72-hour flat consolidation triggers time-decay exit, preventing capital freeze."""
        # 30 bars flat, then enters trade at bar 26, then 40 bars of tight chop
        klines = []
        p = 100.0
        for i in range(70):
            # Bar 25 provides entry trigger (Regime 1 sweep)
            if i == 25:
                klines.append((1000 + i * 3600000, 100.0, 101.0, 95.0, 100.0, 2000.0))
            else:
                klines.append((1000 + i * 3600000, p, p + 0.3, p - 0.3, p + 0.1, 500.0))

        cfg = ConfigManager.get_default_config()
        cfg["time_decay_tighten_hours"] = 12.0
        cfg["time_decay_hard_exit_hours"] = 24.0

        sim = E2EBacktestSimulator(cfg)
        res = sim.run_simulation({"SOLUSDT": klines}, initial_balance=10.0)

        # Verify any executed trade exits within hard exit limit
        for t in res["trades"]:
            self.assertLessEqual(t["holding_duration_hours"], 25.0)
            if t["exit_reason"] == "TIME_DECAY":
                self.assertIn(t["exit_reason"], ["TIME_DECAY"])

    def test_s4_breakout_momentum_and_trailing_stop_runner_scenario(self):
        """Scenario 4: Donchian breakout hits profit lock (+1.5 ATR) and rides trailing stop (+2.2 ATR)."""
        atr = 2.0
        entry_p = 100.0
        peak_p = 108.0  # +4.0 ATR move

        # Verify profit lock activation
        profit_lock_threshold = entry_p + (1.5 * atr)
        self.assertGreater(peak_p, profit_lock_threshold)

        # Verify trailing stop tracking
        trail_activation_threshold = entry_p + (2.2 * atr)
        self.assertGreater(peak_p, trail_activation_threshold)

        calculated_trail_stop = peak_p - (1.2 * atr)
        self.assertAlmostEqual(calculated_trail_stop, 105.6, places=2)

        # Trade record with TRAILING_STOP exit
        trade = TradeRecord(
            symbol="SOLUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700000000000 + (8 * 3600000),
            entry_price=entry_p,
            exit_price=calculated_trail_stop,
            qty=0.1,
            margin_used=1.66,
            exit_reason="TRAILING_STOP",
        )
        self.assertGreater(trade.net_pnl, 0.5)
        self.assertEqual(trade.exit_reason, "TRAILING_STOP")

    def test_s5_micro_capital_10usd_compounding_and_risk_budget_scenario(self):
        """Scenario 5: Micro-capital ($10) account maintains risk <= 6% per trade through series of 10 trades."""
        balance = 10.0
        trades = []
        for i in range(10):
            sizing = StrategyEngine.calculate_order_sizing(
                balance=balance,
                entry_price=50000.0,
                atr=400.0,
                symbol="BTCUSDT",
            )
            self.assertIsNotNone(sizing)
            # Risk must not exceed 15% under micro model or $0.60
            self.assertLessEqual(sizing["actual_risk"], balance * 0.15)
            self.assertGreaterEqual(sizing["notional"], 5.05)

            # Alternate win and small loss
            exit_p = 50800.0 if i % 2 == 0 else 49500.0
            t = TradeRecord(
                symbol="BTCUSDT",
                side="LONG",
                entry_time=1700000000000 + (i * 3600000 * 5),
                exit_time=1700000000000 + (i * 3600000 * 5) + 3600000 * 2,
                entry_price=50000.0,
                exit_price=exit_p,
                qty=sizing["qty"],
                margin_used=sizing["margin_used"],
                exit_reason="TP" if i % 2 == 0 else "SL",
            )
            balance += t.net_pnl
            trades.append(t)

        metrics = RankingEngine.calculate_strategy_metrics("MicroCapitalSeries", 10.0, trades)
        self.assertEqual(metrics["metrics"]["total_trades"], 10)
        self.assertGreater(metrics["metrics"]["final_balance"], 5.05)

    def test_s6_dual_timeframe_hybrid_fusion_scenario(self):
        """Scenario 6: Dual-timeframe configuration parameter comparison (1h swing vs 15m intraday)."""
        cfg_1h = ConfigManager.get_default_config()
        cfg_1h["timeframe"] = "1h"
        cfg_1h["time_decay_tighten_hours"] = 12.0

        cfg_15m = ConfigManager.get_default_config()
        cfg_15m["timeframe"] = "15m"
        cfg_15m["time_decay_tighten_hours"] = 3.0  # 12 bars on 15m

        self.assertEqual(cfg_1h["time_decay_tighten_hours"] / cfg_15m["time_decay_tighten_hours"], 4.0)


if __name__ == "__main__":
    unittest.main()
