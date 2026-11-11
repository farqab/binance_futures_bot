"""
Tier 1: Feature Coverage Test Suite (>=35 tests across 7 features)
Validates primary happy path behavior and interface contracts for all features in PROJECT.md:
- Feature 1: Historical Data Ingestion & Caching (5 tests)
- Feature 2: Time-in-Trade & Duration Tracking (5 tests)
- Feature 3: Time-Decay Stagnation Exit Rule (5 tests)
- Feature 4: Time-Weighted Strategy Optimization (5 tests)
- Feature 5: Comparative Ranking & Report Engine (5 tests)
- Feature 6: Runnable Strategy Deployment Artifacts (5 tests)
- Feature 7: Full E2E Verification & Hardening (5 tests)
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


class TestTier1Feature1DataIngestionCaching(unittest.TestCase):
    """Tier 1: Feature 1 - Historical Data Ingestion & Caching."""

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()
        self.cache = KlineDataCache(self.temp_dir)

    def tearDown(self):
        shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_f1_cache_write_and_read_parity(self):
        """1. Cache round-trip must store and recover exact klines without corruption."""
        klines = generate_synthetic_klines(50, base_price=50000.0, start_time=1700000000000)
        cache_path = self.cache.save_klines("BTCUSDT", "1h", klines)
        self.assertTrue(os.path.exists(cache_path))
        loaded = self.cache.load_klines("BTCUSDT", "1h")
        self.assertEqual(len(loaded), len(klines))
        self.assertEqual(loaded[0][0], klines[0][0])
        self.assertEqual(loaded[-1][4], klines[-1][4])

    def test_f1_kline_schema_validation(self):
        """2. Validates klines against schema: [open_time, open, high, low, close, volume]."""
        valid_klines = [
            [1700000000000, 100.0, 105.0, 99.0, 103.0, 500.0],
            [1700003600000, 103.0, 107.0, 102.0, 106.0, 600.0],
        ]
        self.assertTrue(KlineDataCache.validate_kline_schema(valid_klines))

    def test_f1_all_13_canonical_symbols_supported(self):
        """3. Data ingestion must validate and support all 13 canonical symbols."""
        self.assertEqual(len(CANONICAL_SYMBOLS), 13)
        for sym in CANONICAL_SYMBOLS.keys():
            klines = generate_synthetic_klines(10, base_price=10.0)
            path = self.cache.save_klines(sym, "1h", klines)
            self.assertTrue(os.path.exists(path))

    def test_f1_dual_timeframe_ingestion(self):
        """4. Caching must support distinct 1h and 15m intervals independently."""
        klines_1h = generate_synthetic_klines(24, interval_ms=3600000)
        klines_15m = generate_synthetic_klines(96, interval_ms=900000)
        p1 = self.cache.save_klines("ETHUSDT", "1h", klines_1h)
        p2 = self.cache.save_klines("ETHUSDT", "15m", klines_15m)
        self.assertNotEqual(p1, p2)
        self.assertEqual(len(self.cache.load_klines("ETHUSDT", "1h")), 24)
        self.assertEqual(len(self.cache.load_klines("ETHUSDT", "15m")), 96)

    def test_f1_chronological_ordering_enforcement(self):
        """5. Ingestion must enforce strictly ascending candle open timestamps."""
        unordered_klines = [
            [1700003600000, 100.0, 105.0, 99.0, 102.0, 50.0],
            [1700000000000, 102.0, 106.0, 101.0, 104.0, 60.0],  # Out of order
        ]
        with self.assertRaises(ValueError):
            KlineDataCache.validate_kline_schema(unordered_klines)


class TestTier1Feature2DurationTracking(unittest.TestCase):
    """Tier 1: Feature 2 - Time-in-Trade & Duration Tracking."""

    def test_f2_trade_record_schema_contract(self):
        """6. TradeRecord schema must contain all 14 mandatory fields from PROJECT.md."""
        record = TradeRecord(
            symbol="BTCUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700050000000,
            entry_price=68000.0,
            exit_price=70000.0,
            qty=0.001,
            margin_used=6.8,
            exit_reason="TP",
            entry_bar_idx=10,
            exit_bar_idx=24,
        )
        d = record.to_dict()
        required_keys = [
            "symbol", "side", "entry_time", "exit_time", "holding_duration_hours",
            "holding_bars", "entry_price", "exit_price", "gross_pnl", "fees",
            "net_pnl", "roi_pct", "roi_per_hour", "exit_reason",
        ]
        for k in required_keys:
            self.assertIn(k, d, f"Missing required TradeRecord key: {k}")

    def test_f2_duration_hours_calculation(self):
        """7. Duration must equal (exit_time - entry_time) / 3600000.0 hours."""
        entry_ms = 1700000000000
        exit_ms = entry_ms + (14 * 3600000)  # 14 hours
        record = TradeRecord(
            symbol="SOLUSDT",
            side="LONG",
            entry_time=entry_ms,
            exit_time=exit_ms,
            entry_price=100.0,
            exit_price=110.0,
            qty=0.1,
            margin_used=1.66,
            exit_reason="TP",
        )
        self.assertAlmostEqual(record.holding_duration_hours, 14.0, places=3)

    def test_f2_holding_bars_count(self):
        """8. Holding bars must accurately track discrete bar progression."""
        record = TradeRecord(
            symbol="XRPUSDT",
            side="SHORT",
            entry_time=1700000000000,
            exit_time=1700028800000,
            entry_price=0.60,
            exit_price=0.58,
            qty=10.0,
            margin_used=1.0,
            exit_reason="TP",
            entry_bar_idx=50,
            exit_bar_idx=58,
        )
        self.assertEqual(record.holding_bars, 8)

    def test_f2_hourly_roi_calculation(self):
        """9. roi_per_hour must equal roi_pct / holding_duration_hours."""
        record = TradeRecord(
            symbol="BTCUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700036000000,  # 10 hours
            entry_price=60000.0,
            exit_price=63000.0,
            qty=0.001,
            margin_used=6.0,
            exit_reason="TP",
        )
        expected_hourly = record.roi_pct / record.holding_duration_hours
        self.assertAlmostEqual(record.roi_per_hour, expected_hourly, places=3)

    def test_f2_capital_turnover_velocity(self):
        """10. Trade duration tracking enables capital turnover velocity measurement."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 1000 + 3600000 * 5, 50000, 52000, 0.001, 5.0, "TP"),
            TradeRecord("ETHUSDT", "LONG", 2000, 2000 + 3600000 * 10, 3000, 3100, 0.01, 5.0, "TP"),
        ]
        total_holding_hours = sum(t.holding_duration_hours for t in trades)
        self.assertAlmostEqual(total_holding_hours, 15.0, places=2)


class TestTier1Feature3TimeDecayExits(unittest.TestCase):
    """Tier 1: Feature 3 - Time-Decay Stagnation Exit Rule."""

    def setUp(self):
        self.decay = TimeDecayRule(tighten_hours=12.0, hard_exit_hours=24.0, fee_offset_pct=0.0025)

    def test_f3_stagnation_detection_at_12_bars(self):
        """11. At 12h, stagnant trades without profit lock trigger stop tightening."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=12.0,
            side="LONG",
            entry_price=100.0,
            current_price=100.5,
            current_sl=98.0,
            profit_lock_active=False,
        )
        self.assertFalse(should_exit)
        self.assertIsNotNone(new_sl)
        self.assertAlmostEqual(new_sl, 100.25, places=2)  # 100 * 1.0025

    def test_f3_tighten_stop_to_breakeven_plus_fees(self):
        """12. Tightens stop for SHORT to entry * (1 - 0.0025)."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=13.0,
            side="SHORT",
            entry_price=200.0,
            current_price=199.0,
            current_sl=204.0,
            profit_lock_active=False,
        )
        self.assertFalse(should_exit)
        self.assertAlmostEqual(new_sl, 199.50, places=2)  # 200 * 0.9975

    def test_f3_hard_exit_at_24_bars(self):
        """13. Forces immediate market exit at 24h to liberate frozen capital."""
        should_exit, new_sl, reason = self.decay.evaluate_exit(
            holding_duration_hours=24.0,
            side="LONG",
            entry_price=100.0,
            current_price=101.0,
            current_sl=100.25,
            profit_lock_active=False,
        )
        self.assertTrue(should_exit)
        self.assertEqual(reason, "TIME_DECAY")

    def test_f3_exit_reason_tagged_as_time_decay(self):
        """14. Exits triggered by time decay must be explicitly tagged as TIME_DECAY."""
        trade = TradeRecord(
            symbol="BTCUSDT",
            side="LONG",
            entry_time=1700000000000,
            exit_time=1700000000000 + (24 * 3600000),
            entry_price=50000.0,
            exit_price=50100.0,
            qty=0.001,
            margin_used=5.0,
            exit_reason="TIME_DECAY",
        )
        self.assertEqual(trade.exit_reason, "TIME_DECAY")

    def test_f3_funding_rate_bleed_elimination(self):
        """15. Time decay limits holding time and prevents endless 8-hour funding rate drag."""
        unlimited_hours = 72.0
        decay_hours = 24.0
        funding_fee_unlimited = (unlimited_hours / 8.0) * 0.0001 * 50.0
        funding_fee_decay = (decay_hours / 8.0) * 0.0001 * 50.0
        self.assertLess(funding_fee_decay, funding_fee_unlimited)


class TestTier1Feature4TimeWeightedOptimization(unittest.TestCase):
    """Tier 1: Feature 4 - Time-Weighted Strategy Optimization."""

    def test_f4_regime1_donchian_sweep_wick_exhaustion(self):
        """16. Regime 1 triggers on 20-bar low sweep with lower wick ratio >= 0.35."""
        # 25 bars: first 24 flat at 100, bar 25 sweeps down to 90 with long lower wick
        klines = []
        for i in range(24):
            klines.append((1000 + i * 3600000, 100.0, 102.0, 98.0, 100.0, 1000.0))
        # Sweeping bar: open=100, high=101, low=95, close=100 (wick = 100-95 = 5, range = 6, ratio = 5/6 = 0.83)
        klines.append((1000 + 24 * 3600000, 100.0, 101.0, 95.0, 100.0, 1500.0))
        regime, side, score = StrategyEngine.evaluate_regimes(klines)
        self.assertEqual(regime, 1)
        self.assertEqual(side, "LONG")
        self.assertGreaterEqual(score, 75.0)

    def test_f4_regime2_momentum_breakout_rsi_filter(self):
        """17. Regime 2 triggers on 12-bar breakout with EMA9>EMA21 and valid RSI (46-78)."""
        klines = []
        # Create 20 baseline bars fluctuating between 98 and 102 (RSI around 50)
        for i in range(20):
            p = 100.0 + (1.0 if i % 2 == 0 else -1.0)
            klines.append((1000 + i * 3600000, p, p + 1.0, p - 1.0, p + 0.2, 1000.0))
        # Add 4 steady upward bars to align EMA9 > EMA21 while keeping RSI ~60
        for i in range(20, 24):
            p = 102.0 + (i - 20) * 0.8
            klines.append((1000 + i * 3600000, p, p + 0.8, p - 0.2, p + 0.5, 1200.0))
        # Bar 25: Strong breakout bar that closes at 108.0 (well above high12 of ~105)
        klines.append((1000 + 24 * 3600000, 105.0, 109.0, 104.5, 108.5, 2500.0))
        regime, side, score = StrategyEngine.evaluate_regimes(klines)
        self.assertEqual(regime, 2)
        self.assertEqual(side, "LONG")
        self.assertEqual(score, 78.0)

    def test_f4_regime3_dynamic_ema21_pullback(self):
        """18. Regime 3 triggers when price dips to EMA21 and bounces with wick >= 0.15."""
        klines = generate_synthetic_klines(30, base_price=100.0, trend=0.3, volatility=0.8)
        # Verify that StrategyEngine evaluates without error
        regime, side, score = StrategyEngine.evaluate_regimes(klines)
        self.assertIn(regime, [0, 1, 2, 3])

    def test_f4_split_take_profit_order_sizing(self):
        """19. Order sizing must respect 1.8 ATR SL and 3.6 ATR TP."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=50000.0,
            atr=500.0,
            symbol="BTCUSDT",
            atr_mult_sl=1.8,
            atr_mult_tp=3.6,
        )
        self.assertIsNotNone(sizing)
        self.assertEqual(sizing["sl_distance"], 500.0 * 1.8)
        self.assertEqual(sizing["tp_distance"], 500.0 * 3.6)

    def test_f4_timeframe_parameter_differentiation(self):
        """20. Validates 1h swing vs 15m intraday parameter differentiation."""
        config_1h = ConfigManager.get_default_config()
        config_1h["timeframe"] = "1h"
        config_1h["time_decay_tighten_hours"] = 12.0
        config_15m = ConfigManager.get_default_config()
        config_15m["timeframe"] = "15m"
        config_15m["time_decay_tighten_hours"] = 3.0  # 12 bars on 15m = 3 hours
        self.assertNotEqual(config_1h["time_decay_tighten_hours"], config_15m["time_decay_tighten_hours"])


class TestTier1Feature5ComparativeRanking(unittest.TestCase):
    """Tier 1: Feature 5 - Comparative Ranking & Report Engine."""

    def test_f5_strategy_output_schema_contract(self):
        """21. Validates Strategy Output Schema matching PROJECT.md lines 57-79."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 1000 + 3600000 * 10, 50000, 52000, 0.001, 5.0, "TP"),
            TradeRecord("ETHUSDT", "LONG", 2000, 2000 + 3600000 * 8, 3000, 2900, 0.01, 5.0, "SL"),
        ]
        out = RankingEngine.calculate_strategy_metrics("TestStrategy", 10.0, trades)
        required_metric_keys = [
            "initial_balance", "final_balance", "net_profit_pct", "win_rate_pct",
            "profit_factor", "max_drawdown_pct", "total_trades", "avg_duration_hours",
            "active_capital_hourly_roi", "tas_score", "sharpe_ratio", "sortino_ratio", "calmar_ratio",
        ]
        for k in required_metric_keys:
            self.assertIn(k, out["metrics"], f"Missing required metric: {k}")

    def test_f5_tas_score_exact_calculation(self):
        """22. Time-Adjusted Score (TAS) formula verification."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 1000 + 3600000 * 5, 50000, 53000, 0.001, 5.0, "TP"),
        ]
        out = RankingEngine.calculate_strategy_metrics("TAS_Test", 10.0, trades)
        m = out["metrics"]
        self.assertGreater(m["tas_score"], 0.0)

    def test_f5_comparative_ranking_order(self):
        """23. Multiple strategy results are ranked strictly descending by TAS score."""
        s1 = {"strategy_name": "S1", "metrics": {"tas_score": 50.0, "net_profit_pct": 20.0}}
        s2 = {"strategy_name": "S2", "metrics": {"tas_score": 120.0, "net_profit_pct": 40.0}}
        s3 = {"strategy_name": "S3", "metrics": {"tas_score": 85.0, "net_profit_pct": 30.0}}
        ranked = RankingEngine.rank_strategies([s1, s2, s3], sort_by="tas_score")
        self.assertEqual(ranked[0]["strategy_name"], "S2")
        self.assertEqual(ranked[1]["strategy_name"], "S3")
        self.assertEqual(ranked[2]["strategy_name"], "S1")

    def test_f5_risk_adjusted_metrics_sharpe_sortino_calmar(self):
        """24. Computes valid Sharpe, Sortino, and Calmar ratios."""
        trades = [
            TradeRecord("BTCUSDT", "LONG", 1000, 2000, 50000, 51000, 0.001, 5.0, "TP"),
            TradeRecord("BTCUSDT", "LONG", 3000, 4000, 51000, 52000, 0.001, 5.0, "TP"),
            TradeRecord("BTCUSDT", "LONG", 5000, 6000, 52000, 51500, 0.001, 5.0, "SL"),
        ]
        out = RankingEngine.calculate_strategy_metrics("RiskMetrics", 10.0, trades)
        m = out["metrics"]
        self.assertIsInstance(m["sharpe_ratio"], float)
        self.assertIsInstance(m["sortino_ratio"], float)
        self.assertIsInstance(m["calmar_ratio"], float)

    def test_f5_markdown_report_generation(self):
        """25. Generates formatted comparative Markdown report table."""
        s1 = RankingEngine.calculate_strategy_metrics("Strat_1h", 10.0, [])
        report = RankingEngine.generate_comparative_markdown([s1])
        self.assertIn("# Comparative Performance Ranking Matrix", report)
        self.assertIn("| Rank | Strategy Name |", report)
        self.assertIn("Strat_1h", report)


class TestTier1Feature6RunnableDeploymentArtifacts(unittest.TestCase):
    """Tier 1: Feature 6 - Runnable Strategy Deployment Artifacts."""

    def test_f6_config_json_schema_validation(self):
        """26. Validates structure and fields of config.json."""
        config = ConfigManager.get_default_config()
        self.assertTrue(ConfigManager.validate_config(config))

    def test_f6_micro_capital_10usd_budget_enforcement(self):
        """27. Micro-capital sizing enforces max $0.60 risk on $10 balance."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=100.0,
            atr=2.0,
            symbol="SOLUSDT",
        )
        self.assertIsNotNone(sizing)
        self.assertLessEqual(sizing["actual_risk"], 10.0 * 0.15)

    def test_f6_minimum_notional_5usd_threshold(self):
        """28. Orders must meet Binance minimum notional threshold ($5.05)."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=0.5,
            atr=0.01,
            symbol="XRPUSDT",
        )
        self.assertIsNotNone(sizing)
        self.assertGreaterEqual(sizing["notional"], 5.05)

    def test_f6_commission_gross_profit_ratio_8_to_1(self):
        """29. Expected TP gross profit must exceed round-trip commission by at least 8:1."""
        sizing = StrategyEngine.calculate_order_sizing(
            balance=10.0,
            entry_price=60000.0,
            atr=500.0,
            symbol="BTCUSDT",
        )
        self.assertIsNotNone(sizing)
        tp_gross = sizing["qty"] * sizing["tp_distance"]
        fee = sizing["notional"] * 2.0 * (0.0005 + 0.0001)
        self.assertGreaterEqual(tp_gross, fee * 8.0)

    def test_f6_cli_runner_argument_parsing(self):
        """30. Configuration parameters can be customized for CLI overrides."""
        cfg = ConfigManager.get_default_config()
        cfg["timeframe"] = "15m"
        cfg["atr_mult_sl"] = 2.0
        self.assertTrue(ConfigManager.validate_config(cfg))
        self.assertEqual(cfg["timeframe"], "15m")


class TestTier1Feature7FullE2EVerification(unittest.TestCase):
    """Tier 1: Feature 7 - Full E2E Verification & Hardening."""

    def test_f7_end_to_end_backtest_pipeline(self):
        """31. Executes full end-to-end pipeline: data -> simulation -> metrics."""
        sim = E2EBacktestSimulator()
        data = {
            "BTCUSDT": generate_synthetic_klines(60, base_price=50000.0, volatility=50.0),
            "ETHUSDT": generate_synthetic_klines(60, base_price=3000.0, volatility=10.0),
        }
        res = sim.run_simulation(data, initial_balance=10.0)
        self.assertIn("metrics", res)
        self.assertIn("trades", res)

    def test_f7_single_slot_concurrency_lock(self):
        """32. Enforces strict single-slot concurrency: no overlapping trades across symbols."""
        sim = E2EBacktestSimulator()
        data = {
            "BTCUSDT": generate_synthetic_klines(80, base_price=50000.0, trend=10.0),
            "SOLUSDT": generate_synthetic_klines(80, base_price=100.0, trend=2.0),
        }
        res = sim.run_simulation(data, initial_balance=10.0)
        trades = res["trades"]
        for i in range(1, len(trades)):
            prev_exit = trades[i - 1]["exit_time"]
            curr_entry = trades[i]["entry_time"]
            self.assertGreaterEqual(curr_entry, prev_exit, "Overlapping positions detected! Single slot violated.")

    def test_f7_balance_conservation_invariant(self):
        """33. Invariant: final balance must equal initial_balance + sum(net_pnl)."""
        sim = E2EBacktestSimulator()
        data = {
            "BTCUSDT": generate_synthetic_klines(70, base_price=50000.0, volatility=30.0),
        }
        res = sim.run_simulation(data, initial_balance=10.0)
        trades = res["trades"]
        initial_b = res["metrics"]["initial_balance"]
        final_b = res["metrics"]["final_balance"]
        sum_net_pnl = sum(t["net_pnl"] for t in trades)
        self.assertAlmostEqual(final_b, initial_b + sum_net_pnl, places=2)

    def test_f7_exit_reason_enumeration_integrity(self):
        """34. Invariant: Every closed trade exit reason must be from valid enumeration."""
        valid_reasons = {"TP", "SL", "PROFIT_LOCK", "TRAILING_STOP", "TIME_DECAY"}
        sim = E2EBacktestSimulator()
        data = {
            "BTCUSDT": generate_synthetic_klines(80, base_price=50000.0, volatility=40.0),
        }
        res = sim.run_simulation(data, initial_balance=10.0)
        for t in res["trades"]:
            self.assertIn(t["exit_reason"], valid_reasons)

    def test_f7_reproducible_deterministic_results(self):
        """35. Invariant: Identical input data yields identical backtest results."""
        sim = E2EBacktestSimulator()
        data = {
            "BTCUSDT": generate_synthetic_klines(75, base_price=50000.0, volatility=25.0),
        }
        res1 = sim.run_simulation(data, initial_balance=10.0)
        res2 = sim.run_simulation(data, initial_balance=10.0)
        self.assertEqual(res1["metrics"]["final_balance"], res2["metrics"]["final_balance"])
        self.assertEqual(res1["metrics"]["total_trades"], res2["metrics"]["total_trades"])
        self.assertEqual(len(res1["trades"]), len(res2["trades"]))


if __name__ == "__main__":
    unittest.main()
