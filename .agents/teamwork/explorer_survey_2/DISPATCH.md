## 2026-11-06T16:32:10Z
From: 4cd38888-59b8-4580-a42f-c9475ff72425
Content:
You are explorer_survey_2, a teamwork_explorer subagent.

Your working directory is: E:\binance_futures_bot\.agents\teamwork\explorer_survey_2
Your parent is: 4cd38888-59b8-4580-a42f-c9475ff72425 (use this conversation ID to send_message upon completion)

CRITICAL INSTRUCTION:
You MUST read E:\binance_futures_bot\.agents\teamwork\ORIGINAL_REQUEST.md before starting any work.

Objective:
Map and survey all existing trading strategies, indicator implementations, entry/exit logic, parameter sets, timeframe assumptions, and strategy execution engine/runner in E:\binance_futures_bot.
Specifically:
1. Enumerate all strategy classes/files in E:\binance_futures_bot (e.g. trend following, scalping, mean reversion, breakout, funding rate, etc.).
2. Document the indicators, triggers, stop-loss, take-profit, and risk management logic used by each strategy.
3. Analyze the strategy runner/engine architecture: how strategies are configured, parameterized, instantiated, and executed (live or backtest).
4. Identify which strategies have proven performance or weaknesses based on the code and existing tests.

Scope Boundaries:
- Read-only exploration agent.
- DO NOT modify source code files or create implementation code.
- Write your analysis and handoff only into your working directory: E:\binance_futures_bot\.agents\teamwork\explorer_survey_2.

Output Requirements:
- Write detailed analysis to E:\binance_futures_bot\.agents\teamwork\explorer_survey_2\analysis.md
- Write structured handoff report to E:\binance_futures_bot\.agents\teamwork\explorer_survey_2\handoff.md with Observation, Logic Chain, Caveats, Conclusion, and Verification Method.
- Update progress.md regularly with timestamp as your heartbeat.
- When finished, use send_message to notify parent (4cd38888-59b8-4580-a42f-c9475ff72425) with the path to your handoff.md.
