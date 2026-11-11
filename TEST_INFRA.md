# E2E Test Infrastructure & Test Philosophy

**Project:** Binance Futures Bot Time-Based ROI Strategy Optimization  
**Working Directory:** `E:\binance_futures_bot`  
**Test Location:** `E:\binance_futures_bot\tests\`  
**Test Suite Architect:** `test_writer_e2e_1` (teamwork_test_writer)  
**Derivation Sources:** `ORIGINAL_REQUEST.md` (Requirements R1, R2, R3) and `PROJECT.md` (Features 1–7)

---

## 1. Test Philosophy: 4-Tier Opaque-Box Methodology

The test suite follows the rigorous **4-Tier Test Case Design Methodology** designed to ensure zero-regression, deterministic verification, and full coverage of the time-based ROI optimization pipeline without modifying production trading logic.

| Tier | Focus | Minimum Count | Actual Implemented | Target Scope |
|------|-------|---------------|-------------------|--------------|
| **Tier 1** | Feature Coverage | $\ge 35$ (5 per feature) | **35 tests** | Primary happy-path behaviors and interface contracts across all 7 features in `PROJECT.md`. |
| **Tier 2** | Boundary & Corner Cases | $\ge 35$ (5 per feature) | **35 tests** | Zero duration, empty inputs, extreme volatility, division-by-zero clamping, fee drag floors, and account ruin limits. |
| **Tier 3** | Cross-Feature Combinations | $\ge 7$ pairwise | **8 tests** | Pairwise interactions between caching, time-decay, split-TP, single-slot concurrency, fee drag, and ranking matrices. |
| **Tier 4** | Real-World Application Scenarios | $\ge 5$ end-to-end | **6 tests** | Full-scale 10-day multi-symbol backtests (3,120 candles), high-beta BTC lead-lag, sideways chop, and micro-capital compounding. |
| **Total** | Full E2E Test Suite | $\ge 82$ tests | **84 tests** | 100% Pass Rate across all test suites. |

---

## 2. Feature Inventory Mapping (Tier 1 & Tier 2)

Each of the 7 features defined in `PROJECT.md` is covered by at least 5 Tier 1 functional tests and 5 Tier 2 boundary tests:

### Feature 1: Historical Data Ingestion & Caching
* **Description:** Local caching of 13-symbol klines (1h, 15m) to eliminate network latency and ensure reproducible backtests.
* **Tier 1 Tests (`test_tier1_features.py`):**
  1. `test_f1_cache_write_and_read_parity`: Round-trip persistence and byte-level recovery.
  2. `test_f1_kline_schema_validation`: Strict validation of `[open_time, o, h, l, c, v]`.
  3. `test_f1_all_13_canonical_symbols_supported`: Support for all 13 canonical pairs (`BTC, ETH, XRP, SOL, DOGE, LINK, SUI, AVAX, NEAR, ADA, APT, DOT, TIA`).
  4. `test_f1_dual_timeframe_ingestion`: Independent caching of 1h and 15m intervals.
  5. `test_f1_chronological_ordering_enforcement`: Validation of strictly ascending timestamps.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  1. `test_b1_empty_kline_dataset`: Zero-length klines handled gracefully.
  2. `test_b1_corrupted_json_cache_recovery`: Malformed cache recovery.
  3. `test_b1_zero_volume_doji_candles`: Zero-volume candles handled without division-by-zero.
  4. `test_b1_extreme_flash_crash_gap`: 90% flash crash candles validated safely.
  5. `test_b1_duplicate_timestamp_rejection`: Backward/duplicate timestamps rejected.

### Feature 2: Time-in-Trade & Duration Tracking
* **Description:** Instrumentation to measure trade entry timestamp, bar count, holding duration, and hourly capital turnover adhering to the M1 ↔ M2 interface contract.
* **Tier 1 Tests (`test_tier1_features.py`):**
  6. `test_f2_trade_record_schema_contract`: 14 mandatory fields from `PROJECT.md:37–54`.
  7. `test_f2_duration_hours_calculation`: Exact $(t_{\text{exit}} - t_{\text{entry}}) / 3600000.0$ calculation.
  8. `test_f2_holding_bars_count`: Discrete bar counter incrementation.
  9. `test_f2_hourly_roi_calculation`: Hourly ROI = $\text{roi\_pct} / \text{duration\_hours}$.
  10. `test_f2_capital_turnover_velocity`: Active holding hours vs idle time tracking.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  6. `test_b2_zero_duration_trade`: Trade opened and closed in same millisecond.
  7. `test_b2_extreme_long_duration_trade`: 1,000+ hour holding without precision loss.
  8. `test_b2_exact_breakeven_trade_zero_net_pnl`: Zero net PnL yields 0.0% ROI cleanly.
  9. `test_b2_sub_hour_duration_precision`: 15-minute holding (0.25h) scales ROI by 4x.
  10. `test_b2_negative_duration_rejection`: Negative elapsed time raises `ValueError`.

### Feature 3: Time-Decay Stagnation Exit Rule
* **Description:** Dynamic exit rules to tighten stops to breakeven + commission (+0.25%) at 12h and force market close at 24h to liberate frozen capital.
* **Tier 1 Tests (`test_tier1_features.py`):**
  11. `test_f3_stagnation_detection_at_12_bars`: Stop tightening trigger at 12 hours.
  12. `test_f3_tighten_stop_to_breakeven_plus_fees`: Move SL to `entry * 1.0025` (Long) or `entry * 0.9975` (Short).
  13. `test_f3_hard_exit_at_24_bars`: Forced market exit at 24 hours.
  14. `test_f3_exit_reason_tagged_as_time_decay`: Explicit `TIME_DECAY` tagging.
  15. `test_f3_funding_rate_bleed_elimination`: Reduction in 8-hour funding fee accumulation.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  11. `test_b3_trade_exits_at_bar_11_escapes_decay`: Trade at 11.9h escapes time decay.
  12. `test_b3_exact_bar_12_tighten_trigger`: Stop tightens at exactly 12.0h.
  13. `test_b3_exact_bar_24_hard_exit_trigger`: Forced exit triggers at exactly 24.0h.
  14. `test_b3_tightened_stop_already_exceeded_by_gap`: Down-gap crossing tightened stop triggers immediate exit.
  15. `test_b3_consecutive_time_decay_liquidations`: Clean sequence of 5 stagnant exits.

### Feature 4: Time-Weighted Strategy Optimization
* **Description:** Tri-Regime signal engine (Regime 1: Donchian Sweeps, Regime 2: Momentum Breakout, Regime 3: EMA21 Pullback) and Split Take-Profit.
* **Tier 1 Tests (`test_tier1_features.py`):**
  16. `test_f4_regime1_donchian_sweep_wick_exhaustion`: 20-bar sweep with wick ratio $\ge 0.35$.
  17. `test_f4_regime2_momentum_breakout_rsi_filter`: 12-bar Donchian breakout with EMA9>EMA21 and RSI (46–78).
  18. `test_f4_regime3_dynamic_ema21_pullback`: EMA21 touch and bounce with wick $\ge 0.15$.
  19. `test_f4_split_take_profit_order_sizing`: Split TP sizing at 1.8 ATR and 3.6 ATR.
  20. `test_f4_timeframe_parameter_differentiation`: 1h vs 15m parameter presets.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  16. `test_b4_ultra_high_atr_size_rejection`: ATR risk > 15% rejected safely.
  17. `test_b4_zero_atr_flatline_market`: Flat market (0.0 ATR) returns `None` safely.
  18. `test_b4_exact_confluence_threshold_boundary`: Strict 75.0 score boundary.
  19. `test_b4_split_tp_indivisible_lot_size`: Step size quantization for discrete coins (DOGE, ADA).
  20. `test_b4_sudden_regime_flip_during_trade`: Mid-trade indicator flips do not corrupt position state.

### Feature 5: Comparative Ranking & Report Engine
* **Description:** Computing Net Profit %, ROI / unit time, Time-Adjusted Score (TAS), Sharpe, Sortino, Calmar, and generating Markdown ranking matrices.
* **Tier 1 Tests (`test_tier1_features.py`):**
  21. `test_f5_strategy_output_schema_contract`: 13 mandatory metrics from `PROJECT.md:57–79`.
  22. `test_f5_tas_score_exact_calculation`: Verification of Time-Adjusted Score formula.
  23. `test_f5_comparative_ranking_order`: Descending sort by TAS score.
  24. `test_f5_risk_adjusted_metrics_sharpe_sortino_calmar`: Calculation of Sharpe, Sortino, Calmar.
  25. `test_f5_markdown_report_generation`: Markdown table formatting.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  21. `test_b5_zero_trades_metric_defaults`: Zero trades handled with 0.0 defaults.
  22. `test_b5_100_percent_winrate_profit_factor_cap`: Profit factor clamped to 99.0 on 0 losses.
  23. `test_b5_0_percent_winrate_tas_score_zero`: 0% win rate yields 0.0 TAS score.
  24. `test_b5_zero_drawdown_denominator_clamping`: 0% drawdown clamped to 1.0 denominator.
  25. `test_b5_fees_exceed_gross_profit_fee_factor_zero`: Negative fee factor clamped to 0.0.

### Feature 6: Runnable Strategy Deployment Artifacts
* **Description:** Validating `config.json` deployment schemas, micro-capital $10 budget limits, and CLI runner commands.
* **Tier 1 Tests (`test_tier1_features.py`):**
  26. `test_f6_config_json_schema_validation`: Mandatory keys and parameter ranges.
  27. `test_f6_micro_capital_10usd_budget_enforcement`: 6% ($0.60) risk cap enforcement.
  28. `test_f6_minimum_notional_5usd_threshold`: Binance $5.05 minimum notional validation.
  29. `test_f6_commission_gross_profit_ratio_8_to_1`: 8:1 gross profit to fee coverage ratio.
  30. `test_f6_cli_runner_argument_parsing`: CLI configuration flag overrides.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  26. `test_b6_missing_required_config_field_validation`: Missing required config keys raise `ValueError`.
  27. `test_b6_negative_risk_budget_rejection`: Negative risk budgets raise `ValueError`.
  28. `test_b6_excessive_leverage_clamp`: Unsupported symbol in config raises `ValueError`.
  29. `test_b6_insufficient_balance_under_5usd`: Account balance < $5.05 prevents order placement.
  30. `test_b6_empty_symbols_list_rejection`: Empty symbol list in config raises `ValueError`.

### Feature 7: Full E2E Verification & Hardening
* **Description:** Validating end-to-end backtesting pipeline, balance conservation, and single-slot concurrency locks.
* **Tier 1 Tests (`test_tier1_features.py`):**
  31. `test_f7_end_to_end_backtest_pipeline`: Complete data -> simulation -> metrics flow.
  32. `test_f7_single_slot_concurrency_lock`: Zero overlapping positions across all symbols.
  33. `test_f7_balance_conservation_invariant`: $\text{Balance}_{\text{final}} = \text{Balance}_{\text{initial}} + \sum \text{Net PnL}$.
  34. `test_f7_exit_reason_enumeration_integrity`: Exits restricted strictly to valid enum.
  35. `test_f7_reproducible_deterministic_results`: Repeated runs yield identical balances.
* **Tier 2 Boundary Tests (`test_tier2_boundaries.py`):**
  31. `test_b7_maximum_drawdown_account_ruin_halt`: Account balance < $5.05 halts trading safely.
  32. `test_b7_multi_signal_tie_breaker`: Deterministic tie-breaking on simultaneous signals.
  33. `test_b7_rapid_sl_hit_on_entry_bar`: Wick touching SL on entry bar handles exit cleanly.
  34. `test_b7_extreme_slippage_stress_test`: Accounting stability under 10% adverse slippage.
  35. `test_b7_cross_day_boundary_timestamping`: Seamless handling across UTC calendar day boundaries.

---

## 3. Cross-Feature Combinations (Tier 3)

Located in `tests/test_tier3_cross_features.py`:
1. `test_c1_data_cache_hit_to_duration_tracker_integration`: Data cache hit feeds directly into backtest simulation; trade durations match cached timestamps.
2. `test_c2_time_decay_exit_to_tas_ranking_comparison`: Enabling time-decay exits reduces stagnation duration and improves TAS / hourly ROI compared to unlimited hold.
3. `test_c3_split_tp_with_profit_lock_and_trailing`: Profit lock (+1.5 ATR) moves SL to breakeven; subsequent trailing stop (+2.2 ATR) trails peak price.
4. `test_c4_single_slot_concurrency_and_time_decay_unlock`: Stagnant trade on Coin A closed by time decay immediately frees single slot for Coin B on next bar.
5. `test_c5_fee_drag_threshold_and_time_decay_breakeven`: Across all 13 canonical symbols, time-decay breakeven stop covers round-trip fees (0.12%).
6. `test_c6_multi_strategy_parameter_sweep_to_comparative_ranking_matrix`: Multi-parameter backtest sweep ranks into unified Markdown matrix.
7. `test_c7_config_json_override_to_full_e2e_pipeline`: Overrides in `config.json` directly alter trade exit distributions.
8. `test_c8_btc_lead_lag_boost_to_altcoin_sizing_interaction`: BTC momentum boost elevates sub-threshold altcoin signal into execution.

---

## 4. Real-World Application Scenarios (Tier 4)

Located in `tests/test_tier4_real_world.py`:
1. `test_s1_full_10day_13symbol_backtest_scenario`: 10-day backtest across all 13 canonical pairs (3,120 candles) verifying portfolio balance and report generation.
2. `test_s2_high_beta_btc_lead_lag_propagation_scenario`: BTC +2.5% sharp rally propagates to SOL, capturing follow-through momentum.
3. `test_s3_sideways_consolidation_stagnation_decay_scenario`: 72-hour flat consolidation triggers time-decay exit, preventing capital freeze and 8h funding rate bleed.
4. `test_s4_breakout_momentum_and_trailing_stop_runner_scenario`: Regime 2 momentum breakout hits profit lock (+1.5 ATR) and rides trailing stop (+2.2 ATR) to exit.
5. `test_s5_micro_capital_10usd_compounding_and_risk_budget_scenario`: $10 starting account compounds across 10 trades while strictly respecting the 6% risk cap.
6. `test_s6_dual_timeframe_hybrid_fusion_scenario`: 1h swing trend + 15m intraday entries parameter fusion comparison.

---

## 5. How to Run the Tests

### Option A: Unified Standalone Runner (Recommended)
```powershell
python -X utf8 E:\binance_futures_bot\tests\run_all_e2e.py
```
* **Exit code:** 0 on 100% pass, 1 on failure.
* **Output:** Full breakdown of all 4 Tiers, pass/fail counts, execution time, and summary table.

### Option B: Pytest Test Runner
```powershell
pytest E:\binance_futures_bot\tests\
```

### Option C: Python Unittest Standard Library
```powershell
python -X utf8 -m unittest discover -s E:\binance_futures_bot\tests -p "test_*.py"
```
