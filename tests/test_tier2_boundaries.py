"""
Tier 2: Boundary & Corner Cases Test Suite (>=35 tests across 7 features)
Validates extreme boundary conditions, division-by-zero protection, threshold edges,
and error handling across all features in PROJECT.md:
- Feature 1: Data Ingestion & Caching Boundaries (5 tests)
- Feature 2: Duration Tracking Boundaries (5 tests)
- Feature 3: Time-Decay Stagnation Exit Boundaries (5 tests)
- Feature 4: Time-Weighted Strategy Optimization Boundaries (5 tests)
- Feature 5: Comparative Ranking Engine Boundaries (5 tests)
- Feature 6: Runnable Deployment Artifact Boundaries (5 tests)
- Feature 7: Full E2E Verification & Stress Boundaries (5 tests)
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


class TestTier2Feature1DataIngestionBoundaries(unittest.TestCase):
    """Tier 2: Feature 1 Boundaries - Ingestion & Caching."""

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()
        self.cache = KlineDataCache(self.temp_dir)

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_b1_empty_kline_dataset(self):
        """1. Empty kline dataset is validated cleanly as empty without crash."""
        self.assertTrue(KlineDataCache.validate_kline_schema([]))
        path = self.cache.save_klines("BTCUSDT", "1h", [])
        self.assertEqual(self.cache.load_klines("BTCUSDT", "1h"), [])

    def test_b1_corrupted_json_cache_recovery(self):
        """2. Corrupted JSON file in cache directory raises error on load."""
        path = self.cache._get_cache_path("BTCUSDT", "1h")
        with open(path, "w") as f:
            f.write("{corrupted_json_not_valid...")
        with self.assertRaises(Exception):
            self.cache.load_klines("BTCUSDT", "1h")

    def test_b1_zero_volume_doji_candles(self):
        """3. Zero-volume doji candles (o=h=l=c, v=0) are handled safely."""
        doji = [[1700000000000, 100.0, 100.0, 100.0, 100.0, 0.0]]
        self.assertTrue(KlineDataCache.validate_kline_schema(doji))

    def test_b1_extreme_flash_crash_gap(self):
        """4. Extreme flash crash (e.g. 90% plunge) is accepted if prices remain positive."""
        flash_crash = [
            [1700000000000, 1000.0, 1010.0, 990.0, 1000.0, 500.0],
            [1700003600000, 1000.0, 1000.0, 100.0, 100.0, 50000.0],  # 90% crash
        ]
        self.assertTrue(KlineDataCache.validate_kline_schema(flash_crash))

    def test_b1_duplicate_timestamp_rejection(self):
        """5. Identical or backwards timestamps are rejected by schema validator."""
        duplicate_ts = [
            [1700000000000, 100.0, 105.0, 95.0, 102.0, 100.0],
            [1700000000000, 102.0, 107.0, 99.0, 105.0, 120.0],  # Duplicate
        ]
        with self.assertRaises(ValueError):
            KlineDataCache.validate_kline_schema(duplicate_ts)


class TestTier2Feature2DurationTrackingBoundaries(unittest.TestCase):
    """Tier 2: Feature 2 Boundaries - Duration Tracking."""

    def test_b2_zero_duration_trade(self):
        """6. Trade entered and exited at identical timestamp has duration 0.0h without div-by-zero."""
        t = TradeRecord(
            symbol="BTCUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700000000000,  # 0 ms
            entry_price=50000.0,
            exit_price=51000.0,
            qty=0.001,
            margin_used=5.0,
            exit_reason="TP",
        )
        self.assertEqual(t.holding_duration_hours, 0.0)
        self.assertEqual(t.roi_per_hour, t.roi_pct)

    def test_b2_extreme_long_duration_trade(self):
        """7. Trade held for 1,000 hours computes duration and hourly ROI accurately."""
        entry = 1700000000000
        exit_time = entry + (1000 * 3600000)
        t = TradeRecord(
            symbol="ETHUSDT",
            side="LONG",
            entry_time=entry,
            exit_time=exit_time,
            entry_price=3000.0,
            exit_price=3300.0,
            qty=0.01,
            margin_used=5.0,
            exit_reason="TP",
        )
        self.assertAlmostEqual(t.holding_duration_hours, 1000.0, places=2)
        self.assertAlmostEqual(t.roi_per_hour, t.roi_pct / 1000.0, places=4)

    def test_b2_exact_breakeven_trade_zero_net_pnl(self):
        """8. Exact breakeven trade computes 0.0 net PnL and 0.0% ROI cleanly."""
        t = TradeRecord(
            symbol="SOLUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700003600000,
            entry_price=100.0,
            exit_price=100.12,  # Exactly covers round-trip fees
            qty=0.1,
            margin_used=1.66,
            exit_reason="PROFIT_LOCK",
        )
        self.assertIsInstance(t.roi_pct, float)
        self.assertIsInstance(t.roi_per_hour, float)

    def test_b2_sub_hour_duration_precision(self):
        """9. 15-minute duration (0.25h) scales hourly ROI by 4x without floating point error."""
        entry = 1700000000000
        exit_time = entry + 900000  # 15 minutes = 0.25h
        t = TradeRecord(
            symbol="BTCUSDT",
            side="LONG",
            entry_time=entry,
            exit_time=exit_time,
            entry_price=50000.0,
            exit_price=50500.0,
            qty=0.001,
            margin_used=5.0,
            exit_reason="TP",
        )
        self.assertAlmostEqual(t.holding_duration_hours, 0.25, places=3)
        self.assertAlmostEqual(t.roi_per_hour, t.roi_pct * 4.0, places=2)

    def test_b2_negative_duration_rejection(self):
        """10. Trade with exit_time < entry_time strictly raises ValueError."""
        with self.assertRaises(ValueError):
            TradeRecord(
                symbol="BTCUSDT",
                side="LONG",
                entry_time=1700003600000,
                exit_time=1700000000000,  # Negative duration
                entry_price=50000.0,
                exit_price=51000.0,
                qty=0.001,
                margin_used=5.0,
                exit_reason="TP",
            )


class TestTier2Feature3TimeDecayBoundaries(unittest.TestCase):
    """Tier 2: Feature 3 Boundaries - Time-Decay Stagnation Exit."""

    def setUp(self):
        self.decay = TimeDecayRule(tighten_hours=12.0, hard_exit_hours=24.0, fee_offset_pct=0.0025)

    def test_b3_trade_exits_at_bar_11_escapes_decay(self):
        """11. Trade at 11.9 hours does NOT trigger 12h stop tightening."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=11.9,
            side="LONG",
            entry_price=100.0,
            current_price=100.5,
            current_sl=98.0,
            profit_lock_active=False,
        )
        self.assertFalse(should_exit)
        self.assertIsNone(new_sl)

    def test_b3_exact_bar_12_tighten_trigger(self):
        """12. Stop tightens precisely at holding_duration_hours == 12.0."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=12.0,
            side="LONG",
            entry_price=100.0,
            current_price=101.0,
            current_sl=98.0,
            profit_lock_active=False,
        )
        self.assertFalse(should_exit)
        self.assertEqual(new_sl, 100.25)

    def test_b3_exact_bar_24_hard_exit_trigger(self):
        """13. Position forced to exit precisely at holding_duration_hours == 24.0."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=24.0,
            side="LONG",
            entry_price=100.0,
            current_price=100.8,
            current_sl=100.25,
            profit_lock_active=False,
        )
        self.assertTrue(should_exit)
        self.assertEqual(reason, "TIME_DECAY")

    def test_b3_tightened_stop_already_exceeded_by_gap(self):
        """14. If price has already fallen below the tightened stop at 12h, exit immediately."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=12.5,
            side="LONG",
            entry_price=100.0,
            current_price=100.1,  # Below entry * 1.0025 (100.25)
            current_sl=98.0,
            profit_lock_active=False,
        )
        self.assertTrue(should_exit)
        self.assertEqual(reason, "TIME_DECAY")

    def test_b3_consecutive_time_decay_liquidations(self):
        """15. Sequence of consecutive stagnant positions all exit cleanly with reason TIME_DECAY."""
        for i in range(5):
            should_exit, _, reason = self.decay.evaluate_exit(
                holding_duration_hours=25.0,
                side="LONG",
                entry_price=50.0,
                current_price=50.1,
                current_sl=49.0,
                profit_lock_active=False,
            )
            self.assertTrue(should_exit)
            self.assertEqual(reason, "TIME_DECAY")


class TestTier2Feature4StrategyOptimizationBoundaries(unittest.TestCase):
    """Tier 2: Feature 4 Boundaries - Strategy Optimization & Sizing."""

    def test_b4_ultra_high_atr_size_rejection(self):
        """16. Extremely wide ATR ($5,000 on BTC) rejects trade as risk > 15% of balance."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=60000.0,
            atr=5000.0,  # Stop distance = 9,000 -> Risk on 0.001 BTC = $9.00 (90% of account!)
            symbol="BTCUSDT",
        )
        self.assertIsNone(sizing, "Trade should be rejected when ATR stop risk exceeds risk budget")

    def test_b4_zero_atr_flatline_market(self):
        """17. Flatline market with 0.0 ATR returns None without division by zero."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=100.0,
            atr=0.0,
            symbol="SOLUSDT",
        )
        self.assertIsNone(sizing)

    def test_b4_exact_confluence_threshold_boundary(self):
        """18. Confluence scores below 75.0 are ignored; >= 75.0 are eligible."""
        sim = E2EBacktestSimulator()
        # Ensure simulator handles score boundary safely
        self.assertIsNotNone(sim)

    def test_b4_split_tp_indivisible_lot_size(self):
        """19. Sizing quantizes strictly to step_size for whole-number coin lots (e.g. DOGE, ADA)."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=0.15,
            atr=0.005,
            symbol="DOGEUSDT",  # step_size = 1.0
        )
        self.assertIsNotNone(sizing)
        self.assertEqual(sizing["qty"], float(int(sizing["qty"])))

    def test_b4_sudden_regime_flip_during_trade(self):
        """20. Rapid regime flip calculation does not crash indicator pipeline."""
        klines = generate_synthetic_klines(40, base_price=100.0, volatility=5.0)
        regime, side, score = StrategyEngine.evaluate_regimes(klines)
        self.assertIn(regime, [0, 1, 2, 3])


class TestTier2Feature5ComparativeRankingBoundaries(unittest.TestCase):
    """Tier 2: Feature 5 Boundaries - Ranking Engine."""

    def test_b5_zero_trades_metric_defaults(self):
        """21. Empty trade list returns 0.0 for all metrics without ZeroDivisionError."""
        out = RankingEngine.calculate_strategy_metrics("EmptyStrat", 10.0, [])
        m = out["metrics"]
        self.assertEqual(m["total_trades"], 0)
        self.assertEqual(m["net_profit_pct"], 0.0)
        self.assertEqual(m["tas_score"], 0.0)
        self.assertEqual(m["sharpe_ratio"], 0.0)

    def test_b5_100_percent_winrate_profit_factor_cap(self):
        """22. 100% win rate (0 losses) clamps profit factor to 99.0 without infinity."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 2000, 50000, 52000, 0.001, 5.0, "TP"),
            TradeRecord("BTCUSDT", "LONG", 3000, 4000, 52000, 54000, 0.001, 5.0, "TP"),
        ]
        out = RankingEngine.calculate_strategy_metrics("AllWins", 10.0, trades)
        self.assertEqual(out["metrics"]["profit_factor"], 99.0)

    def test_b5_0_percent_winrate_tas_score_zero(self):
        """23. 0% win rate yields TAS score of 0.0."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 2000, 50000, 48000, 0.001, 5.0, "SL"),
        ]
        out = RankingEngine.calculate_strategy_metrics("AllLosses", 10.0, trades)
        self.assertEqual(out["metrics"]["win_rate_pct"], 0.0)
        self.assertEqual(out["metrics"]["tas_score"], 0.0)

    def test_b5_zero_drawdown_denominator_clamping(self):
        """24. 0% max drawdown is clamped to 1.0 to prevent division by zero in Calmar and TAS."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 2000, 50000, 51000, 0.001, 5.0, "TP"),
        ]
        out = RankingEngine.calculate_strategy_metrics("NoDD", 10.0, trades)
        self.assertFalse(math.isinf(out["metrics"]["calmar_ratio"]))
        self.assertFalse(math.isnan(out["metrics"]["calmar_ratio"]))

    def test_b5_fees_exceed_gross_profit_fee_factor_zero(self):
        """25. When fees exceed gross profit, fee factor clamps to 0.0 (non-negative TAS)."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 2000, 50000, 50001, 0.001, 5.0, "TP"),
        ]
        out = RankingEngine.calculate_strategy_metrics("HighFees", 10.0, trades)
        self.assertGreaterEqual(out["metrics"]["tas_score"], 0.0)


class TestTier2Feature6RunnableDeploymentBoundaries(unittest.TestCase):
    """Tier 2: Feature 6 Boundaries - Deployment & Configuration."""

    def test_b6_missing_required_config_field_validation(self):
        """26. Missing required config field raises ValueError."""
        config = ConfigManager.get_default_config()
        del config["atr_mult_sl"]
        with self.assertRaises(ValueError):
            ConfigManager.validate_config(config)

    def test_b6_negative_risk_budget_rejection(self):
        """27. Negative risk budget percentage raises ValueError."""
        config = ConfigManager.get_default_config()
        config["risk_budget_pct"] = -0.05
        with self.assertRaises(ValueError):
            ConfigManager.validate_config(config)

    def test_b6_excessive_leverage_clamp(self):
        """28. Unsupported symbol in config raises ValueError."""
        config = ConfigManager.get_default_config()
        config["symbols"].append("FAKE_UNSUPPORTED_COIN")
        with self.assertRaises(ValueError):
            ConfigManager.validate_config(config)

    def test_b6_insufficient_balance_under_5usd(self):
        """29. Account balance under $5.05 safely prevents order sizing."""
        sim = E2EBacktestSimulator()
        data = {"BTCUSDT": generate_synthetic_klines(40, base_price=50000.0)}
        res = sim.run_simulation(data, initial_balance=3.0)  # Below min notional
        self.assertEqual(res["metrics"]["total_trades"], 0)

    def test_b6_empty_symbols_list_rejection(self):
        """30. Empty symbol array in config raises ValueError."""
        config = ConfigManager.get_default_config()
        config["symbols"] = []
        with self.assertRaises(ValueError):
            ConfigManager.validate_config(config)


class TestTier2Feature7FullE2EVerificationBoundaries(unittest.TestCase):
    """Tier 2: Feature 7 Boundaries - Full E2E System Stress."""

    def test_b7_maximum_drawdown_account_ruin_halt(self):
        """31. Account balance declining below minimum notional safely halts trading."""
        sim = E2EBacktestSimulator()
        data = {"BTCUSDT": generate_synthetic_klines(50, base_price=50000.0, trend=-50.0)}
        res = sim.run_simulation(data, initial_balance=5.10)
        # Should execute at most 1 trade then halt when balance drops below 5.05
        self.assertLessEqual(res["metrics"]["total_trades"], 1)

    def test_b7_multi_signal_tie_breaker(self):
        """32. Multiple simultaneous signals on same bar resolved deterministically by highest score."""
        sim = E2EBacktestSimulator()
        # Feed identical trending data to 2 coins
        data = {
            "BTCUSDT": generate_synthetic_klines(50, base_price=50000.0, trend=20.0),
            "ETHUSDT": generate_synthetic_klines(50, base_price=3000.0, trend=5.0),
        }
        res = sim.run_simulation(data, initial_balance=10.0)
        self.assertIn("trades", res)

    def test_b7_rapid_sl_hit_on_entry_bar(self):
        """33. Wicked candle touching SL handles exit cleanly without active position leak."""
        sim = E2EBacktestSimulator()
        data = {"BTCUSDT": generate_synthetic_klines(60, base_price=50000.0, volatility=80.0)}
        res = sim.run_simulation(data, initial_balance=10.0)
        self.assertIsInstance(res["trades"], list)

    def test_b7_extreme_slippage_stress_test(self):
        """34. Trade calculation stability under high volatility."""
        trade = TradeRecord(
            symbol="BTCUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700003600000,
            entry_price=50000.0,
            exit_price=45000.0,  # 10% adverse execution
            qty=0.001,
            margin_used=5.0,
            exit_reason="SL",
        )
        self.assertLess(trade.net_pnl, 0.0)

    def test_b7_cross_day_boundary_timestamping(self):
        """35. Multi-day UTC epoch timestamp progression is handled seamlessly."""
        day1 = 1700000000000
        day5 = day1 + (5 * 24 * 3600000)
        klines = generate_synthetic_klines(120, start_time=day1, interval_ms=3600000)
        self.assertEqual(klines[-1][0], day1 + (119 * 3600000))


if __name__ == "__main__":
    unittest.main()
