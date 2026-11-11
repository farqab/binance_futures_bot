# Task Assignment: Milestone 2 Technical Exploration
Working directory: E:\binance_futures_bot\.agents\teamwork\explorer_m2_1
Original request file: E:\binance_futures_bot\.agents\teamwork\ORIGINAL_REQUEST.md
Project plan: E:\binance_futures_bot\PROJECT.md
Milestone 1 Report: E:\binance_futures_bot\reports\backtest_latest.json

Scope:
Formulate the architecture and parameter space for Milestone 2:
1. **Feature 3 - Time-Decay Stagnation Exit Rule**:
   - Design dynamic time-decay rules:
     - 12h/12-bar stagnation rule: if price has not reached +1.0x ATR profit after 12 bars, tighten stop loss to breakeven + round-trip commission (`entry_price * 1.0025` for longs).
     - 24h/24-bar hard stagnation exit: if position remains open after 24 bars without hitting TP, close at market to free the single-slot concurrency lock for higher-conviction opportunities.
2. **Feature 4 - Time-Weighted Multi-Strategy Optimization Engine**:
   - Design a modular multi-strategy evaluation framework across:
     - Base Tri-Regime v8.3 (1h candles)
     - Tri-Regime with Time-Decay Exit (1h candles)
     - Tri-Regime with Split-TP (50% at 1.8 ATR, 50% at 3.6 ATR + Time-Decay)
     - High-Velocity Intraday Scalping (15m candles with tight 6-bar decay)
     - Multi-Timeframe Confluence (1h trend + 15m execution trigger)
   - Define exact optimization metrics: Net PnL %, ROI per hour, TAS score, Win Rate %, Profit Factor, Max Drawdown %, and average duration.
Write findings and concrete specifications in handoff.md.

## 2026-11-06T17:20:41Z
You are explorer_m2_1, a teamwork_explorer subagent.

Your working directory is: E:\binance_futures_bot\.agents\teamwork\explorer_m2_1
Your parent is: 4cd38888-59b8-4580-a42f-c9475ff72425 (use this conversation ID to send_message upon completion)

CRITICAL INSTRUCTION:
You MUST read E:\binance_futures_bot\.agents\teamwork\ORIGINAL_REQUEST.md, E:\binance_futures_bot\PROJECT.md, and E:\binance_futures_bot\reports\backtest_latest.json before starting work.

Objective:
Formulate the architecture and parameter space for Milestone 2 (Features 3 & 4):
1. **Feature 3 - Time-Decay Stagnation Exit Rule**:
   - Design dynamic time-decay rules:
     - 12h/12-bar stagnation rule: if price has not reached +1.0x ATR profit after 12 bars, tighten stop loss to breakeven + round-trip commission (`entry_price * 1.0025` for longs).
     - 24h/24-bar hard stagnation exit: if position remains open after 24 bars without hitting TP, close at market to free the single-slot concurrency lock for higher-conviction opportunities.
   - Specify boundary conditions, edge cases, and mathematical proof of capital turnover gain.
2. **Feature 4 - Time-Weighted Multi-Strategy Optimization Framework**:
   - Design a modular optimization framework in `strategies/time_decay_optimizer.py` and/or `src/main.rs` that loads all 13 cached symbols from `data/cache/` (offline and instant) and benchmarks:
     - Strategy 1: Tri-Regime v8.3 Baseline (1h, no time decay)
     - Strategy 2: Tri-Regime + Time-Decay Stagnation Exit (1h, 12h BE / 24h hard exit)
     - Strategy 3: Tri-Regime + Split-TP (50% at 1.8 ATR, 50% at 3.6 ATR + Time-Decay)
     - Strategy 4: High-Velocity Scalping (15m candles with 6-bar decay)
     - Strategy 5: Dynamic Volatility-Scaled Time-Decay (scaling exit bars inversely with ATR%)
   - Define exact formula for Time-Adjusted Score (TAS) and comparative metric ranking:
     `TAS = (Net_PnL% / Max_DD%) * (WinRate% / 50%) * (Hourly_Active_ROE / Benchmark_Hourly_ROE) * (1 - Fees/Gross_Wins)`
3. Provide concrete Worker pseudocode, interface types, and verification invariants.

Scope Boundaries:
- Read-only exploration agent. Do NOT write production implementation code.
- Write handoff report in E:\binance_futures_bot\.agents\teamwork\explorer_m2_1\handoff.md.
- Send completion message to parent with summary and path to handoff.md.
