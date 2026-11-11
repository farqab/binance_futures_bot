# Handoff Report: Milestone 1 - Feature 2: Time-in-Trade & Duration Tracking Instrumentation

**Agent**: `explorer_m1_2` (teamwork_explorer)  
**Parent**: `4cd38888-59b8-4580-a42f-c9475ff72425`  
**Milestone**: M1 (Backtest Caching & Duration Tracking Engine) — Feature 2  
**Working Directory**: `E:\binance_futures_bot\.agents\teamwork\explorer_m1_2`  
**Date**: 2026-11-06T16:58:00Z  

---

## 1. Observation

### 1.1 Existing Rust Data Structures (`src/main.rs`)
Inspection of `E:\binance_futures_bot\src\main.rs` revealed:

1. **`ClosedTradeRecord` Struct (Lines 253–267)**:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   struct ClosedTradeRecord {
       pub timestamp: String,
       pub symbol: String,
       pub side: String,
       pub leverage: u32,
       pub qty: f64,
       pub margin_used: f64,
       pub entry_price: f64,
       pub exit_price: f64,
       pub gross_pnl: f64,
       pub fees_paid: f64,
       pub net_pnl: f64,
       pub roe_pct: f64,
       pub reason: String,
   }
   ```
   *Direct Observation*: `ClosedTradeRecord` completely omits `entry_time_ms`, `exit_time_ms`, `entry_bar_idx`, `exit_bar_idx`, `holding_bars`, `holding_duration_hours`, `holding_duration_seconds`, and `hourly_active_roe`. The `timestamp` field (line 254) is populated at exit (line 3385) via `Utc::now().format("%H:%M:%S").to_string()`, discarding date and trade duration.

2. **Live Position State Enum (Lines 269–299)**:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
   #[serde(tag = "side")]
   enum PositionState {
       Flat,
       Long {
           symbol: String,
           leverage: u32,
           entry_price: f64,
           qty: f64,
           margin_used: f64,
           stop_loss: f64,
           take_profit: f64,
           highest_price: f64,
           trailing_active: bool,
           entry_fee: f64,
           opened_at: String,
       },
       Short { ... },
   }
   ```
   *Direct Observation*: Position variants only store human-readable `opened_at: String`. They do not store numeric millisecond timestamps (`entry_time_ms: u64`) or candle indices (`entry_bar_idx: usize`), preventing zero-latency duration math during live state transitions or time-decay triggers.

3. **Backtest Active Position Representation (Lines 1617–1618 & Lines 1977–1988)**:
   ```rust
   let mut active: Option<(String, &'static str, f64, f64, f64, f64, f64, bool, f64, f64)> = None;
   ...
   active = Some((
       eng.spec.symbol.to_string(),
       eng.signal_dir,
       exec_p,
       qty,
       sl,
       tp,
       exec_p,
       false,
       e_fee,
       eng.live_atr14,
   ));
   ```
   *Direct Observation*: The backtest position is an untyped 10-tuple storing: `(symbol, side, entry_price, qty, sl, tp, peak_price, trailing_active, entry_fee, atr)`. It discards `entry_bar_idx`, `entry_time_ms`, and `margin_used`.

4. **Backtest Trade Exit & Accounting (Lines 1764–1814)**:
   At trade exit, the backtest updates aggregate balance, daily stats, and win/loss counters, but does NOT construct or accumulate `ClosedTradeRecord`s. The duration of the trade is completely discarded.

5. **Backtest Summary Metrics (Lines 2029–2059)**:
   The backtest prints total trades, win rate, profit factor, net return %, and calendar daily average (`avg_daily_usd = (balance - INITIAL_BALANCE) / num_days`), but provides zero visibility into average trade duration, capital turnover, hourly ROE, or trade expectancy per hour.

### 1.2 Interface Contract in `PROJECT.md` (Lines 34–79)
`PROJECT.md` explicitly defines the M1 ↔ M2 data contract:
```json
{
  "symbol": "BTCUSDT",
  "side": "LONG",
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
  "exit_reason": "TP"
}
```
And aggregate strategy metrics:
```json
{
  "avg_duration_hours": 9.4,
  "active_capital_hourly_roi": 0.25,
  "tas_score": 142.8
}
```

### 1.3 Test Harness Architecture (`tests/test_harness.py`)
In `tests/test_harness.py`:
- `TradeRecord` (lines 108–195) provides a reference Python implementation enforcing `duration_ms = max(0, exit_time - entry_time)`, `holding_duration_hours = round(duration_ms / 3600000.0, 4)`, and `roi_per_hour = round(roi_pct / holding_duration_hours, 4)`.
- `RankingEngine.calculate_strategy_metrics` (lines 445–545) aggregates `total_duration_hours`, `avg_duration_hours`, and `active_capital_hourly_roi`.
- Execution test verified that `TradeRecord` outputs the exact dictionary keys expected by `PROJECT.md`.

---

## 2. Logic Chain

### 2.1 From Observation 1.1 to Schema Enhancement
1. Because `ClosedTradeRecord` in `src/main.rs:253` currently lacks duration attributes, neither live bot telemetry nor backtest exports can report holding duration or hourly capital efficiency.
2. In order to preserve backward compatibility when loading existing `bot_state.json` files from disk, all newly added fields in `ClosedTradeRecord` MUST be annotated with `#[serde(default)]`. This guarantees zero deserialization failures on existing deployments.
3. In `PositionState::Long` and `PositionState::Short`, adding `entry_time_ms: u64` and `entry_bar_idx: usize` allows the bot to know the precise duration in milliseconds and bars at any tick without re-parsing ISO strings.

### 2.2 From Observation 1.1 (3–5) to Backtest Modernization
1. Because the backtest loop currently uses an untyped 10-tuple `active`, adding fields without strong typing increases regression risk.
2. Therefore, the backtest position should be modeled as an explicit struct `BacktestPosition` (or extended 13-tuple) holding: `entry_time_ms: u64`, `entry_bar_idx: usize`, and `margin_used: f64`.
3. In the exit block (lines 1764–1808), when an exit condition triggers, the engine can compute `duration_ms = exit_time_ms - pos.entry_time_ms`, instantiate a full `ClosedTradeRecord`, and push it to `closed_trades: Vec<ClosedTradeRecord>`.

### 2.3 Mathematical Formulations & Dimensional Consistency

#### Formulation 1: `holding_duration_hours`
$$\text{holding\_duration\_hours} = \frac{t_{\text{exit\_ms}} - t_{\text{entry\_ms}}}{3{,}600{,}000.0}$$
- **Bar Count**: $\text{holding\_bars} = \max(0, \text{bar}_{\text{exit}} - \text{bar}_{\text{entry}})$.
- **Effective Duration (Boundary Protection)**:
  $$\Delta t_{\text{effective\_hours}} = \max\left(\text{holding\_duration\_hours}, \frac{1.0}{60.0}\right)$$
  *Rationale*: A trade closed within seconds (e.g. flash wick stop-out) has $\Delta t \approx 0$. Dividing by zero or near-zero would artificially explode hourly ROE to $>10,000\%$/hr. A 1-minute floor ($1/60 \approx 0.01667$ hours) prevents outlier distortion while preserving economic reality.

#### Formulation 2: `hourly_active_roe`
$$\text{hourly\_active\_roe} = \frac{\text{net\_pnl}}{\text{margin\_used} \times \Delta t_{\text{effective\_hours}}}$$
$$\text{hourly\_active\_roe\_pct} = \frac{\text{roe\_pct}}{\Delta t_{\text{effective\_hours}}}$$
- **Dimensionality**: $[T^{-1}]$ (% return per hour of active capital exposure).
- **Strategy-Level Aggregate Active Capital Hourly ROE**:
  $$\text{active\_capital\_hourly\_roi} = \frac{\text{net\_profit\_pct}}{\max\left(\sum_{k=1}^N \Delta t_{\text{hours}, k}, 1.0\right)}$$
  *Rationale*: Distinguishes between strategies that produce 20% net profit by holding capital captive for 720 hours vs. strategies that achieve 20% in 40 hours of active market exposure.

#### Formulation 3: `capital_turnover_rate`
$$\text{capital\_turnover\_rate} = \frac{\text{total\_notional\_traded}}{\text{initial\_capital} \times \text{total\_duration\_days}}$$
Where:
- $\text{total\_notional\_traded} = \sum_{k=1}^N (\text{entry\_price}_k \cdot \text{qty}_k + \text{exit\_price}_k \cdot \text{qty}_k)$
- $\text{initial\_capital} = \text{INITIAL\_BALANCE}$ (default $\$10.00$)
- $\text{total\_duration\_days} = \max\left(\frac{t_{\text{dataset\_end\_ms}} - t_{\text{dataset\_start\_ms}}}{86{,}400{,}000.0}, \frac{1.0}{24.0}\right)$
- **Dimensionality**: $[\text{days}^{-1}]$ (times initial capital turns over in gross market volume per day).

#### Formulation 4: `trade_expectancy_per_hour`
Standard Expectancy per trade:
$$\mathbb{E}[\text{Trade}] = (P_{\text{win}} \times \overline{\text{Win}}) - (P_{\text{loss}} \times \overline{\text{Loss}})$$
Where:
- $P_{\text{win}} = \frac{N_{\text{wins}}}{N_{\text{total}}}$, $P_{\text{loss}} = \frac{N_{\text{losses}}}{N_{\text{total}}}$
- $\overline{\text{Win}} = \frac{1}{N_{\text{wins}}} \sum_{k \in \text{wins}} \text{net\_pnl}_k$
- $\overline{\text{Loss}} = \frac{1}{N_{\text{losses}}} \sum_{k \in \text{losses}} |\text{net\_pnl}_k|$
Then hourly trade expectancy is:
$$\text{trade\_expectancy\_per\_hour} = \frac{\mathbb{E}[\text{Trade}]}{\overline{\Delta t}_{\text{effective\_hours}}}$$
Where $\overline{\Delta t}_{\text{effective\_hours}} = \frac{1}{N_{\text{total}}} \sum_{k=1}^{N_{\text{total}}} \Delta t_{\text{effective\_hours}, k}$.
- **Dimensionality**: $[\$ / \text{hour}]$ (expected dollar profit generated per active hour of position holding).

---

## 3. Caveats

1. **Exchange Fill Latency in Live Trading**: In live testnet/mainnet, WebSocket order confirmations may have 20–150ms network jitter. Using `entry_time_ms = Utc::now().timestamp_millis()` when the order fill response is received guarantees monotonic non-negative duration.
2. **Candle Open Time vs. Intra-Bar Event Time in Backtesting**: In kline-based backtesting without tick data, entry occurs at candle open ($t_{\text{open}}$). If a TP or SL is hit on the same candle, the exact exit timestamp within that candle is unknown. The standard convention is to assume exit at candle close ($t_{\text{close}} = t_{\text{open}} + \Delta t_{\text{timeframe}} - 1$) or floor duration at 1 full bar.
3. **Multi-Regime Timeframe Differences**: When running on 15m klines, 1 bar = 0.25 hours; on 1h klines, 1 bar = 1.0 hour. Duration in hours must ALWAYS be computed directly from epoch milliseconds, while `holding_bars` tracks the candle count.

---

## 4. Conclusion & Concrete Code Templates

### 4.1 Schema Definitions

#### A. Target Rust Data Structures (`src/main.rs`)

```rust
// ===========================================================================
// Milestone 1 Feature 2: Time-in-Trade & Duration Tracking Schema
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedTradeRecord {
    pub timestamp: String,
    pub symbol: String,
    pub side: String,
    pub leverage: u32,
    pub qty: f64,
    pub margin_used: f64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub gross_pnl: f64,
    pub fees_paid: f64,
    pub net_pnl: f64,
    pub roe_pct: f64,
    pub reason: String,

    // Duration & Time-Metric Instrumentation (Serde Default for 100% Backward Compatibility)
    #[serde(default)]
    pub entry_time_ms: u64,
    #[serde(default)]
    pub exit_time_ms: u64,
    #[serde(default)]
    pub entry_bar_idx: usize,
    #[serde(default)]
    pub exit_bar_idx: usize,
    #[serde(default)]
    pub holding_bars: usize,
    #[serde(default)]
    pub holding_duration_hours: f64,
    #[serde(default)]
    pub holding_duration_seconds: u64,
    #[serde(default)]
    pub hourly_active_roe: f64,
    #[serde(default)]
    pub hourly_active_roe_pct: f64,
    #[serde(default)]
    pub notional_traded: f64,
}

#[derive(Debug, Clone)]
pub struct BacktestPosition {
    pub symbol: String,
    pub side: &'static str,
    pub entry_price: f64,
    pub qty: f64,
    pub margin_used: f64,
    pub leverage: u32,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub peak_price: f64,
    pub trailing_active: bool,
    pub entry_fee: f64,
    pub atr: f64,
    pub entry_time_ms: u64,
    pub entry_bar_idx: usize,
}
```

#### B. Rust Backtest Execution Delta Template (`src/main.rs`)

```rust
// 1. Position Entry inside Backtest Loop (around line 1977)
let margin_used = (exec_p * qty) / (eng.spec.leverage as f64);
let entry_time_ms = raw_candles.get(eng.spec.symbol).unwrap()[i + 1].0;

active = Some(BacktestPosition {
    symbol: eng.spec.symbol.to_string(),
    side: eng.signal_dir,
    entry_price: exec_p,
    qty,
    margin_used,
    leverage: eng.spec.leverage,
    stop_loss: sl,
    take_profit: tp,
    peak_price: exec_p,
    trailing_active: false,
    entry_fee: e_fee,
    atr: eng.live_atr14,
    entry_time_ms,
    entry_bar_idx: i + 1,
});

// 2. Position Exit inside Backtest Loop (around line 1764)
let exit_time_ms = raw_candles.get(&pos.symbol).unwrap()[i].0;
let raw_duration_ms = exit_time_ms.saturating_sub(pos.entry_time_ms);
let holding_duration_hours = (raw_duration_ms as f64) / 3_600_000.0;
let holding_bars = i.saturating_sub(pos.entry_bar_idx);
let effective_duration_hours = holding_duration_hours.max(1.0 / 60.0);

let roe_pct = if pos.margin_used > 0.0 { (net / pos.margin_used) * 100.0 } else { 0.0 };
let hourly_active_roe = if pos.margin_used > 0.0 { net / pos.margin_used / effective_duration_hours } else { 0.0 };
let hourly_active_roe_pct = roe_pct / effective_duration_hours;
let notional_traded = (pos.entry_price + exit_exec) * pos.qty;

closed_trades.push(ClosedTradeRecord {
    timestamp: chrono::DateTime::from_timestamp_millis(exit_time_ms as i64)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "Unknown".to_string()),
    symbol: pos.symbol.clone(),
    side: pos.side.to_string(),
    leverage: pos.leverage,
    qty: pos.qty,
    margin_used: pos.margin_used,
    entry_price: pos.entry_price,
    exit_price: exit_exec,
    gross_pnl: gross,
    fees_paid: pos.entry_fee + exit_fee,
    net_pnl: net,
    roe_pct,
    reason: exit_reason.to_string(),
    entry_time_ms: pos.entry_time_ms,
    exit_time_ms,
    entry_bar_idx: pos.entry_bar_idx,
    exit_bar_idx: i,
    holding_bars,
    holding_duration_hours,
    holding_duration_seconds: raw_duration_ms / 1000,
    hourly_active_roe,
    hourly_active_roe_pct,
    notional_traded,
});

// 3. Backtest Final Summary Instrumentation (around line 2045)
let total_duration_hours: f64 = closed_trades.iter().map(|t| t.holding_duration_hours).sum();
let avg_duration_hours = if total_tr > 0 { total_duration_hours / total_tr as f64 } else { 0.0 };
let active_capital_hourly_roe = if total_duration_hours > 0.0 { ret_pct / total_duration_hours.max(1.0) } else { 0.0 };
let total_notional: f64 = closed_trades.iter().map(|t| t.notional_traded).sum();
let capital_turnover_rate = total_notional / INITIAL_BALANCE / num_days.max(0.04167);

let win_trades: Vec<&ClosedTradeRecord> = closed_trades.iter().filter(|t| t.net_pnl > 0.0).collect();
let loss_trades: Vec<&ClosedTradeRecord> = closed_trades.iter().filter(|t| t.net_pnl <= 0.0).collect();
let avg_win = if !win_trades.is_empty() { win_trades.iter().map(|t| t.net_pnl).sum::<f64>() / win_trades.len() as f64 } else { 0.0 };
let avg_loss = if !loss_trades.is_empty() { loss_trades.iter().map(|t| t.net_pnl.abs()).sum::<f64>() / loss_trades.len() as f64 } else { 0.0 };
let win_rate_dec = if total_tr > 0 { wins as f64 / total_tr as f64 } else { 0.0 };
let loss_rate_dec = if total_tr > 0 { losses as f64 / total_tr as f64 } else { 0.0 };
let trade_expectancy = (win_rate_dec * avg_win) - (loss_rate_dec * avg_loss);
let trade_expectancy_per_hour = if avg_duration_hours > 0.0 { trade_expectancy / avg_duration_hours.max(1.0 / 60.0) } else { 0.0 };

println!(
    "⏱️ [ZAMAN BAZLI VERİMLİLİK METRİKLERİ]\n   Ortalama Pozisyon Süresi: {:.2} saat | Toplam Maruziyet: {:.1} saat\n   Aktif Sermaye Saatlik Getiri: {:+.4}% / saat | Sermaye Devir Hızı: {:.2}x / gün\n   Saatlik Beklenti (Expectancy/h): {:+.4}$ / saat | İşlem Başına Beklenti: {:+.4}$",
    avg_duration_hours, total_duration_hours, active_capital_hourly_roe, capital_turnover_rate, trade_expectancy_per_hour, trade_expectancy
);
```

#### C. Python Duration Tracker Module (`strategies/duration_tracker.py`)

```python
"""
Milestone 1 Feature 2: Time-in-Trade & Duration Tracking Instrumentation.
Provides standardized trade record modeling, duration metrics calculation,
and boundary defenses against division-by-zero or sub-minute trades.
"""

from dataclasses import dataclass, asdict
from typing import List, Dict, Any, Optional

@dataclass
class TradeRecord:
    symbol: str
    side: str
    entry_time_ms: int
    exit_time_ms: int
    entry_price: float
    exit_price: float
    qty: float
    leverage: int
    margin_used: float
    exit_reason: str
    entry_bar_idx: int = 0
    exit_bar_idx: int = 0
    taker_fee_rate: float = 0.0005
    slippage_rate: float = 0.0001

    # Computed fields
    holding_duration_hours: float = 0.0
    holding_bars: int = 0
    holding_duration_seconds: int = 0
    gross_pnl: float = 0.0
    fees: float = 0.0
    net_pnl: float = 0.0
    roe_pct: float = 0.0
    hourly_active_roe: float = 0.0
    roi_per_hour: float = 0.0
    notional_traded: float = 0.0

    def __post_init__(self):
        # 1. Duration math with sub-minute floor boundary protection
        raw_duration_ms = max(0, self.exit_time_ms - self.entry_time_ms)
        self.holding_duration_hours = round(raw_duration_ms / 3600000.0, 4)
        self.holding_bars = max(0, self.exit_bar_idx - self.entry_bar_idx)
        self.holding_duration_seconds = raw_duration_ms // 1000

        # Effective duration floor: minimum 1 minute (1/60 hr) to prevent division by zero
        effective_duration_hours = max(self.holding_duration_hours, 1.0 / 60.0)

        # 2. Financial Gross PnL
        if self.side == "LONG":
            self.gross_pnl = round((self.exit_price - self.entry_price) * self.qty, 4)
        else:
            self.gross_pnl = round((self.entry_price - self.exit_price) * self.qty, 4)

        # 3. Notional & Fee Drag
        notional_entry = self.entry_price * self.qty
        notional_exit = self.exit_price * self.qty
        self.notional_traded = round(notional_entry + notional_exit, 4)
        self.fees = round(self.notional_traded * (self.taker_fee_rate + self.slippage_rate), 4)
        self.net_pnl = round(self.gross_pnl - self.fees, 4)

        # 4. ROE and Active Hourly Rate
        safe_margin = max(self.margin_used, 0.0001)
        self.roe_pct = round((self.net_pnl / safe_margin) * 100.0, 4)
        self.hourly_active_roe = round(self.net_pnl / safe_margin / effective_duration_hours, 6)
        self.roi_per_hour = round(self.roe_pct / effective_duration_hours, 4)

    def to_dict(self) -> Dict[str, Any]:
        """Conforms 100% to PROJECT.md M1 <-> M2 Interface Contract."""
        return {
            "symbol": self.symbol,
            "side": self.side,
            "entry_time": self.entry_time_ms,
            "exit_time": self.exit_time_ms,
            "holding_duration_hours": self.holding_duration_hours,
            "holding_bars": self.holding_bars,
            "entry_price": self.entry_price,
            "exit_price": self.exit_price,
            "gross_pnl": self.gross_pnl,
            "fees": self.fees,
            "net_pnl": self.net_pnl,
            "roi_pct": self.roe_pct,
            "roi_per_hour": self.roi_per_hour,
            "exit_reason": self.exit_reason,
        }


class DurationMetricsCalculator:
    """Calculates aggregate time-based and capital velocity metrics."""

    @staticmethod
    def calculate_metrics(
        trades: List[TradeRecord],
        initial_capital: float = 10.0,
        total_test_days: float = 10.0,
    ) -> Dict[str, Any]:
        if not trades:
            return {
                "total_trades": 0,
                "avg_duration_hours": 0.0,
                "active_capital_hourly_roi": 0.0,
                "capital_turnover_rate": 0.0,
                "trade_expectancy_per_hour": 0.0,
                "trade_expectancy": 0.0,
                "win_rate_pct": 0.0,
                "profit_factor": 0.0,
            }

        total_trades = len(trades)
        total_duration_hours = sum(t.holding_duration_hours for t in trades)
        avg_duration_hours = round(total_duration_hours / total_trades, 2)
        effective_avg_duration = max(avg_duration_hours, 1.0 / 60.0)

        wins = [t for t in trades if t.net_pnl > 0]
        losses = [t for t in trades if t.net_pnl <= 0]
        gross_win = sum(t.gross_pnl for t in wins)
        gross_loss = sum(abs(t.gross_pnl) for t in losses)

        win_rate = len(wins) / total_trades
        loss_rate = len(losses) / total_trades
        avg_win = sum(t.net_pnl for t in wins) / len(wins) if wins else 0.0
        avg_loss = sum(abs(t.net_pnl) for t in losses) / len(losses) if losses else 0.0

        trade_expectancy = round((win_rate * avg_win) - (loss_rate * avg_loss), 4)
        trade_expectancy_per_hour = round(trade_expectancy / effective_avg_duration, 4)

        total_notional = sum(t.notional_traded for t in trades)
        safe_days = max(total_test_days, 1.0 / 24.0)
        capital_turnover_rate = round(total_notional / max(initial_capital, 0.001) / safe_days, 2)

        total_net_pnl = sum(t.net_pnl for t in trades)
        net_profit_pct = (total_net_pnl / max(initial_capital, 0.001)) * 100.0
        active_capital_hourly_roi = round(
            net_profit_pct / max(total_duration_hours, 1.0), 4
        ) if total_duration_hours > 0 else 0.0

        profit_factor = round(gross_win / gross_loss, 2) if gross_loss > 0 else 99.0

        return {
            "total_trades": total_trades,
            "avg_duration_hours": avg_duration_hours,
            "active_capital_hourly_roi": active_capital_hourly_roi,
            "capital_turnover_rate": capital_turnover_rate,
            "trade_expectancy_per_hour": trade_expectancy_per_hour,
            "trade_expectancy": trade_expectancy,
            "win_rate_pct": round(win_rate * 100.0, 2),
            "profit_factor": profit_factor,
        }
```

---

## 5. Verification Method

### 5.1 Verification Commands
The downstream Worker and Reviewer can independently verify these designs with the following test commands:

1. **Rust Compilation Invariance Check**:
   ```powershell
   cargo check --manifest-path E:\binance_futures_bot\Cargo.toml
   ```
   *Expected Result*: Exits 0 with zero compile errors when `ClosedTradeRecord` and `BacktestPosition` are integrated.

2. **Python Duration Tracker Unit Test**:
   ```powershell
   python -X utf8 -c "
   import sys
   sys.path.insert(0, 'E:/binance_futures_bot/tests')
   import test_harness
   t = test_harness.TradeRecord(
       symbol='BTCUSDT',
       side='LONG',
       entry_time=1700000000000,
       exit_time=1700050000000,
       entry_price=68000.0,
       exit_price=70000.0,
       qty=0.001,
       margin_used=10.0,
       exit_reason='TP',
       entry_bar_idx=10,
       exit_bar_idx=24
   )
   data = t.to_dict()
   assert data['holding_duration_hours'] == 13.8889, f'Unexpected duration: {data[\"holding_duration_hours\"]}'
   assert data['roi_per_hour'] == 1.3804, f'Unexpected roi_per_hour: {data[\"roi_per_hour\"]}'
   print('✅ TradeRecord Interface Contract verified!')
   "
   ```
   *Expected Result*: Prints `✅ TradeRecord Interface Contract verified!` with exit code 0.

3. **Boundary Condition Robustness Check**:
   ```powershell
   python -X utf8 -c "
   import sys
   sys.path.insert(0, 'E:/binance_futures_bot/tests')
   import test_harness
   # Zero duration test (exit_time == entry_time)
   t_zero = test_harness.TradeRecord('BTCUSDT', 'LONG', 1700000000000, 1700000000000, 70000, 70100, 0.001, 7.0, 'TP')
   assert t_zero.holding_duration_hours == 0.0
   assert t_zero.roi_per_hour == t_zero.roi_pct
   print('✅ Zero-duration boundary defense passed!')
   "
   ```
   *Expected Result*: Prints `✅ Zero-duration boundary defense passed!` with exit code 0.

### 5.2 Invalidation Conditions
This architecture proposal is invalidated if:
1. Adding duration fields to `ClosedTradeRecord` causes Serde deserialization errors when parsing existing `bot_state.json` (prevented by `#[serde(default)]`).
2. PnL or trade accounting differs by $>0.0001$$ from the baseline backtest (prevented by strict separation of duration telemetry from order execution math).
3. The JSON trade record schema diverges from the M1 ↔ M2 contract in `PROJECT.md:36–54`.
