# Task Assignment: Milestone 1 Technical Exploration - Duration & Time-Metric Tracking
Working directory: E:\binance_futures_bot\.agents\teamwork\explorer_m1_2
Original request file: E:\binance_futures_bot\.agents\teamwork\ORIGINAL_REQUEST.md
Project plan: E:\binance_futures_bot\PROJECT.md

Scope:
Design the implementation strategy for Feature 2: Time-in-Trade & Duration Tracking Instrumentation.
- Design entry timestamp and bar count tracking inside the active position representation.
- Design metric calculation formulas: holding duration (hours/bars), active hourly ROI (`net_pnl / margin / duration_hours`), capital turnover, trade expectancy per unit time.
- Design output data structure for closed trades.
Write findings and recommended architecture in handoff.md.


## 2026-11-06T16:45:27Z
Sender: 4cd38888-59b8-4580-a42f-c9475ff72425
Content:
You are explorer_m1_2, a teamwork_explorer subagent.
Your working directory is: E:\binance_futures_bot\.agents\teamwork\explorer_m1_2
Your parent is: 4cd38888-59b8-4580-a42f-c9475ff72425 (use this conversation ID to send_message upon completion)

CRITICAL INSTRUCTION:
You MUST read E:\binance_futures_bot\.agents\teamwork\ORIGINAL_REQUEST.md and E:\binance_futures_bot\PROJECT.md before starting work.

Objective:
Investigate and design the implementation strategy for Milestone 1 - Feature 2: Time-in-Trade & Duration Tracking Instrumentation.
1. Design the exact schema to track trade entry timestamp, bar count, holding duration (hours/bars), and exit timestamp in the backtest simulator and live trade records.
2. Formulate explicit mathematical definitions for:
   - holding_duration_hours = (exit_time_ms - entry_time_ms) / 3,600,000.0
   - hourly_active_roe = net_pnl / margin_used / holding_duration_hours
   - capital_turnover_rate = total_notional_traded / initial_capital / total_duration_days
   - trade_expectancy_per_hour = (win_rate * avg_win - loss_rate * avg_loss) / avg_duration_hours
3. Provide the Worker with concrete code templates for both Python simulation engines and Rust structs (ClosedTradeRecord with duration fields).
4. Specify boundary checks (e.g. zero duration protection, sub-minute trades).

Scope Boundaries:
- Read-only exploration agent. Do NOT modify production code directly.
- Write your report and recommendations in E:\binance_futures_bot\.agents\teamwork\explorer_m1_2\handoff.md.
- Send completion message to parent (4cd38888-59b8-4580-a42f-c9475ff72425) when finished.
