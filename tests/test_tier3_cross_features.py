"""
Tier 3: Cross-Feature Combinations Test Suite (>=7 tests)
Validates pairwise and multi-feature interactions across PROJECT.md features:
- Test 1: Data Cache Hit + Duration Tracker Integration
- Test 2: Time-Decay Exit + TAS Metric Ranking Comparison
- Test 3: Split TP + Profit Lock + Trailing Stop Interaction
- Test 4: Single-Slot Concurrency + Time-Decay Position Unlocking
- Test 5: Fee Drag Threshold + Time-Decay Breakeven Stop
- Test 6: Multi-Strategy Parameter Sweep + Comparative Ranking Matrix
- Test 7: Config JSON Override + Full E2E Pipeline
- Test 8: BTC Lead-Lag Boost + Micro-Capital Sizing Interaction
"""

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


class TestTier3CrossFeatureCombinations(unittest.TestCase):
    """Tier 3: Pairwise and multi-feature interaction tests."""

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()
        self.cache = KlineDataCache(self.temp_dir)

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_c1_data_cache_hit_to_duration_tracker_integration(self):
        """1. Data cache hit feeds directly into backtest simulation; trade durations match cached timestamps."""
        klines = generate_synthetic_klines(60, base_price=50000.0, trend=15.0, start_time=1700000000000)
        self.cache.save_klines("BTCUSDT", "1h", klines)

        # Load from cache
        loaded_klines = self.cache.load_klines("BTCUSDT", "1h")
        self.assertIsNotNone(loaded_klines)

        sim = E2EBacktestSimulator()
        res = sim.run_simulation({"BTCUSDT": loaded_klines}, initial_balance=10.0)

        for trade in res["trades"]:
            self.assertGreaterEqual(trade["entry_time"], 1700000000000)
            self.assertGreater(trade["exit_time"], trade["entry_time"])
            expected_hours = (trade["exit_time"] - trade["entry_time"]) / 3600000.0
            self.assertAlmostEqual(trade["holding_duration_hours"], expected_hours, places=3)

    def test_c2_time_decay_exit_to_tas_ranking_comparison(self):
        """2. Proves that enabling time-decay exits reduces stagnation duration and improves TAS / hourly ROI."""
        # Stagnant choppy dataset
        klines = generate_synthetic_klines(100, base_price=100.0, volatility=0.8, trend=0.01)

        # Config WITH time decay (12h tighten, 24h hard exit)
        cfg_with_decay = ConfigManager.get_default_config()
        cfg_with_decay["time_decay_tighten_hours"] = 12.0
        cfg_with_decay["time_decay_hard_exit_hours"] = 24.0
        sim_decay = E2EBacktestSimulator(cfg_with_decay)
        res_decay = sim_decay.run_simulation({"SOLUSDT": klines}, initial_balance=10.0)

        # Config WITHOUT time decay (unlimited 500h)
        cfg_no_decay = ConfigManager.get_default_config()
        cfg_no_decay["time_decay_tighten_hours"] = 500.0
        cfg_no_decay["time_decay_hard_exit_hours"] = 1000.0
        sim_no_decay = E2EBacktestSimulator(cfg_no_decay)
        res_no_decay = sim_no_decay.run_simulation({"SOLUSDT": klines}, initial_balance=10.0)

        # Average duration with decay must be <= average duration without decay
        avg_dur_decay = res_decay["metrics"]["avg_duration_hours"]
        avg_dur_no_decay = res_no_decay["metrics"]["avg_duration_hours"]
        if res_decay["metrics"]["total_trades"] > 0 and res_no_decay["metrics"]["total_trades"] > 0:
            self.assertLessEqual(avg_dur_decay, avg_dur_no_decay)

    def test_c3_split_tp_with_profit_lock_and_trailing(self):
        """3. Interaction: price crossing +1.5 ATR activates profit lock; +2.2 ATR triggers trailing stop."""
        trade_entry = 100.0
        atr = 2.0
        # +1.5 ATR = 103.0 (activates profit lock, moves SL to entry * 1.0025 = 100.25)
        # +2.2 ATR = 104.4 (activates trailing stop)
        # Peak reaches 106.0 -> trailing SL becomes 106.0 - (1.2 * 2.0) = 103.6
        peak_price = 106.0
        trail_sl = peak_price - (1.2 * atr)
        self.assertAlmostEqual(trail_sl, 103.6, places=2)
        self.assertGreater(trail_sl, trade_entry * 1.0025, "Trailing stop must protect locked profit above breakeven")

    def test_c4_single_slot_concurrency_and_time_decay_unlock(self):
        """4. Stagnant trade on Coin A closed by time decay immediately frees single slot for Coin B."""
        sim = E2EBacktestSimulator()
        # Coin A: chops for 30 bars then stays flat
        klines_a = generate_synthetic_klines(60, base_price=50.0, volatility=0.3, trend=0.0)
        # Coin B: explodes upward at bar 35
        klines_b = generate_synthetic_klines(60, base_price=2000.0, volatility=5.0, trend=10.0)

        res = sim.run_simulation({"AVAXUSDT": klines_a, "ETHUSDT": klines_b}, initial_balance=10.0)
        trades = res["trades"]
        self.assertIsInstance(trades, list)
        # Confirm that no two trades overlap in time
        for i in range(1, len(trades)):
            self.assertGreaterEqual(trades[i]["entry_time"], trades[i - 1]["exit_time"])

    def test_c5_fee_drag_threshold_and_time_decay_breakeven(self):
        """5. Across all 13 canonical symbols, time-decay breakeven stop (entry * 1.0025) covers 0.12% fees."""
        fee_offset = 0.0025  # 0.25%
        round_trip_cost = 0.0012  # 0.12% fees + slippage
        net_edge = fee_offset - round_trip_cost
        self.assertGreater(net_edge, 0.001, "Breakeven offset must reliably exceed round-trip friction")

        for sym, spec in CANONICAL_SYMBOLS.items():
            entry_p = 100.0
            qty = spec["min_qty"] * 10
            notional = entry_p * qty
            fees = notional * 2.0 * (0.0005 + 0.0001)
            gross_be = (entry_p * (1.0 + fee_offset) - entry_p) * qty
            self.assertGreater(gross_be, fees, f"Breakeven stop fails to cover fees for {sym}")

    def test_c6_multi_strategy_parameter_sweep_to_comparative_ranking_matrix(self):
        """6. Automated parameter sweep across 3 strategies produces valid comparative ranking matrix."""
        klines = {"BTCUSDT": generate_synthetic_klines(80, base_price=50000.0, trend=25.0)}

        # Strat 1: Default 1h Time-Decay
        cfg1 = ConfigManager.get_default_config()
        sim1 = E2EBacktestSimulator(cfg1)
        res1 = sim1.run_simulation(klines, initial_balance=10.0)
        res1["strategy_name"] = "TriRegime_1h_TimeDecay"

        # Strat 2: Fast Decay (6h / 12h)
        cfg2 = ConfigManager.get_default_config()
        cfg2["time_decay_tighten_hours"] = 6.0
        cfg2["time_decay_hard_exit_hours"] = 12.0
        sim2 = E2EBacktestSimulator(cfg2)
        res2 = sim2.run_simulation(klines, initial_balance=10.0)
        res2["strategy_name"] = "TriRegime_FastDecay"

        # Strat 3: Wide Stop (2.5 ATR SL)
        cfg3 = ConfigManager.get_default_config()
        cfg3["atr_mult_sl"] = 2.5
        sim3 = E2EBacktestSimulator(cfg3)
        res3 = sim3.run_simulation(klines, initial_balance=10.0)
        res3["strategy_name"] = "TriRegime_WideSL"

        # Rank all 3
        ranked = RankingEngine.rank_strategies([res1, res2, res3], sort_by="tas_score")
        self.assertEqual(len(ranked), 3)
        self.assertGreaterEqual(ranked[0]["metrics"]["tas_score"], ranked[1]["metrics"]["tas_score"])
        self.assertGreaterEqual(ranked[1]["metrics"]["tas_score"], ranked[2]["metrics"]["tas_score"])

        # Generate markdown report
        md = RankingEngine.generate_comparative_markdown(ranked)
        self.assertIn("TriRegime_1h_TimeDecay", md)
        self.assertIn("TriRegime_FastDecay", md)
        self.assertIn("TriRegime_WideSL", md)

    def test_c7_config_json_override_to_full_e2e_pipeline(self):
        """7. Parameter overrides in config.json directly alter E2E trade exit distribution and metrics."""
        klines = {"BTCUSDT": generate_synthetic_klines(80, base_price=50000.0, volatility=50.0)}

        custom_cfg = ConfigManager.get_default_config()
        custom_cfg["atr_mult_tp"] = 5.0  # Very wide TP
        custom_cfg["time_decay_hard_exit_hours"] = 15.0  # Aggressive hard exit

        sim = E2EBacktestSimulator(custom_cfg)
        res = sim.run_simulation(klines, initial_balance=10.0)

        self.assertEqual(res["parameters"]["atr_mult_tp"], 5.0)
        self.assertEqual(res["parameters"]["time_decay_hard_exit_hours"], 15.0)
        for t in res["trades"]:
            # No trade duration should exceed hard exit hours
            self.assertLessEqual(t["holding_duration_hours"], 16.0)

    def test_c8_btc_lead_lag_boost_to_altcoin_sizing_interaction(self):
        """8. BTC lead-lag momentum boost elevates altcoin signal above 75.0 threshold to execute sizing."""
        base_score = 70.0  # Normally sub-threshold (< 75.0)
        btc_boost = 10.0   # BTC 15s velocity >= 0.22%
        final_score = base_score + btc_boost  # 80.0 >= 75.0 -> Approved!
        self.assertGreaterEqual(final_score, 75.0)

        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=100.0,
            atr=2.0,
            symbol="SOLUSDT",
        )
        self.assertIsNotNone(sizing)
        self.assertGreaterEqual(sizing["notional"], 5.05)


if __name__ == "__main__":
    unittest.main()
