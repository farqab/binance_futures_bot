# Project: Binance Futures Bot Time-Based ROI Strategy Optimization

## Architecture
- **Language & Runtime**: Rust (live trading daemon & native backtest engine in `src/main.rs`) and Python (fast vectorized backtesting, hyper-parameter search, and analytics in `scratch/` & `analysis/`).
- **Core Strategy Pipeline**:
  - Tri-Regime Engine: Donchian Liquidity Sweeps (R1), Momentum Breakouts (R2), EMA21 Pullbacks (R3).
  - Auxiliary Alphas: Lead-Lag BTC propagation, Order Flow / Delta OI, TreeNews WebSocket scout radar.
  - Risk & Sizing: Micro-capital ($10) sizing with 6% risk budget, 1.8x ATR SL, 3.6x ATR TP, +1.5x ATR profit lock, +2.2x ATR trailing stop.
  - Time Optimization Layer: Time-decay stagnation exits (12h/24h), duration tracking, time-weighted ROI metrics, and decoupled JSON configuration.
- **Data Flow**: Binance Futures REST API (`/fapi/v1/klines`) -> Local Cache (`data/cache/` or `scratch/`) -> Multi-Strategy Backtest Engine -> Duration & Time-Metric Analyzer -> Comparative Ranking Matrix -> Runnable Executable Configurations (`config.json` & scripts).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Historical Data Ingestion & Caching | Local caching of 13-symbol klines (1h, 15m) to eliminate network latency and ensure reproducible backtests | M1 | Survey (explorer_survey_1) |
| 2 | Time-in-Trade & Duration Tracking | Instrumentation to measure trade entry timestamp, bar count, holding duration, and hourly capital turnover | M1 | Survey (explorer_survey_1, 3) |
| 3 | Time-Decay Stagnation Exit Rule | Dynamic exit rules to close or tighten stagnant positions at 12h/24h and liberate capital | M2 | Survey (explorer_survey_2, 3) |
| 4 | Time-Weighted Strategy Optimization | Tuning parameters across 1h swing, 15m intraday, split TP (50% at 1.8 ATR, 50% at 3.6 ATR), and regime thresholds | M2 | Survey (explorer_survey_2, 3) |
| 5 | Comparative Ranking & Report Engine | Computing Net Profit %, ROI / unit time, Time-Adjusted Score (TAS), Sharpe, Sortino, Calmar, MaxDD | M3 | Survey (explorer_survey_1, 3) |
| 6 | Runnable Strategy Deployment Artifacts | Production-ready executable strategy codes, parameterized configs (`config.json`), and CLI runners | M4 | Survey (explorer_survey_2, 3) |
| 7 | Full E2E Verification & Hardening | Requirement-driven test suite validating 100% of backtest pipelines, metric formulas, and runnable configs | Final Milestone | Dual Track Acceptance |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Backtest Caching & Duration Tracking Engine | Implement data ingestion, local kline caching, and trade duration measurement infrastructure | none | DONE |
| M2 | Time-Decay & Strategy Optimization Engine | Implement time-decay stagnation exits, split-TP, and hyper-parameter optimization across timeframes | M1 | DONE |
| M3 | Comparative Performance Ranking Matrix | Execute comprehensive multi-strategy backtests, rank all strategies by TAS / ROI-per-time / Net PnL, and generate definitive comparative report | M2 | DONE |
| M4 | Runnable Strategy Codes & Config Deployment | Provide executable strategy codes, JSON configurations, and CLI runner commands in E:\binance_futures_bot | M3 | DONE |
| Final | E2E Test Pass & Adversarial Hardening | Pass 100% of E2E test suite (Tiers 1-4) and adversarial coverage hardening (Tier 5) | M4, TEST_READY | DONE |

## Interface Contracts

### M1 ↔ M2 (Data & Backtest Interface)
- **Input Data Format**: Cached klines in JSON/CSV: `{symbol: string, klines: [[open_time, open, high, low, close, volume, close_time, ...]]}`.
- **Trade Record Structure**:
  ```json
  {
    "symbol": "BTCUSDT",
    "side": "LONG" | "SHORT",
    "entry_time": 1700000000000,
    "exit_time": 1700050000000,
    "holding_duration_hours": 13.88,
    "holding_bars": 14,
    "entry_price": 68000.0,
    "exit_price": 70000.0,
    "gross_pnl": 0.58,
    "fees": 0.04,
    "net_pnl": 0.54,
    "roi_pct": 5.4,
    "roi_per_hour": 0.389,
    "exit_reason": "TP" | "SL" | "PROFIT_LOCK" | "TRAILING_STOP" | "TIME_DECAY"
  }
  ```

### M2 ↔ M3 (Optimization Output to Ranking Matrix)
- **Strategy Output Schema**:
  ```json
  {
    "strategy_name": "TriRegime_v8_TimeDecay_1h",
    "parameters": { ... },
    "trades": [ ... ],
    "metrics": {
      "initial_balance": 10.0,
      "final_balance": 18.45,
      "net_profit_pct": 84.5,
      "win_rate_pct": 64.0,
      "profit_factor": 2.15,
      "max_drawdown_pct": 11.2,
      "total_trades": 36,
      "avg_duration_hours": 9.4,
      "active_capital_hourly_roi": 0.25,
      "tas_score": 142.8,
      "sharpe_ratio": 2.31,
      "sortino_ratio": 3.45,
      "calmar_ratio": 7.54
    }
  }
  ```

### M3 ↔ M4 (Ranking to Deployment Interface)
- **Configuration Contract**: `config.json` specifying default and optimized parameter presets for top-ranking strategies.
- **Execution Script Contract**: Runnable commands (`python run_strategy.py` or `binance_futures_bot --config config.json`) accepting parameter overrides and reporting live or simulated PnL.

## Code Layout
- `src/main.rs`: Core Rust live bot and native backtest engine.
- `strategies/`: Strategy modules and runnable Python/Rust strategy implementations.
- `data/cache/`: Cached historical klines for backtesting.
- `reports/`: Comparative performance reports, metric matrices, and ranking tables.
- `config/`: Parameter configurations (`strategy_config.json`).
- `tests/`: E2E test harness and test suites.
- `.agents/teamwork/`: Agent metadata, handoffs, and coordination logs.
