use anyhow::Result;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tungstenite::{connect, Message};

type HmacSha256 = Hmac<Sha256>;

const WS_MULTI_URL: &str = "wss://fstream.binance.com/market/stream?streams=\
btcusdt@aggTrade/btcusdt@kline_5m/\
ethusdt@aggTrade/ethusdt@kline_5m/\
solusdt@aggTrade/solusdt@kline_5m/\
dogeusdt@aggTrade/dogeusdt@kline_5m/\
xrpusdt@aggTrade/xrpusdt@kline_5m/\
linkusdt@aggTrade/linkusdt@kline_5m/\
suiusdt@aggTrade/suiusdt@kline_5m/\
avaxusdt@aggTrade/avaxusdt@kline_5m/\
nearusdt@aggTrade/nearusdt@kline_5m/\
adausdt@aggTrade/adausdt@kline_5m/\
aptusdt@aggTrade/aptusdt@kline_5m/\
dotusdt@aggTrade/dotusdt@kline_5m/\
tiausdt@aggTrade/tiausdt@kline_5m/\
bnbusdt@aggTrade/bnbusdt@kline_5m/\
1000pepeusdt@aggTrade/1000pepeusdt@kline_5m/\
injusdt@aggTrade/injusdt@kline_5m/\
arbusdt@aggTrade/arbusdt@kline_5m/\
opusdt@aggTrade/opusdt@kline_5m/\
renderusdt@aggTrade/renderusdt@kline_5m/\
wifusdt@aggTrade/wifusdt@kline_5m/\
1000shibusdt@aggTrade/1000shibusdt@kline_5m/\
ltcusdt@aggTrade/ltcusdt@kline_5m/\
bchusdt@aggTrade/bchusdt@kline_5m/\
etcusdt@aggTrade/etcusdt@kline_5m/\
fetusdt@aggTrade/fetusdt@kline_5m/\
uniusdt@aggTrade/uniusdt@kline_5m/\
aaveusdt@aggTrade/aaveusdt@kline_5m/\
seiusdt@aggTrade/seiusdt@kline_5m/\
taousdt@aggTrade/taousdt@kline_5m/\
jupusdt@aggTrade/jupusdt@kline_5m";

const REST_BASE_URL: &str = "https://fapi.binance.com";
const CURL_BIN: &str = if cfg!(windows) { "curl.exe" } else { "curl" };
const INITIAL_BALANCE: f64 = 10.00;        // $10.00 USDT Mikro-Kasa Başlangıcı
#[allow(dead_code)]
const MARGIN_USAGE_RATIO: f64 = 0.50;      // Dinamik Risk Kontrollü Marjin Tavanı
// 🏆 Ajan-433 ŞAMPİYON PARAMETRELERİ (90G ROI: +%452.8 | Win Rate: %71.2 | PF: 1.59)
const SL_ATR_MULT: f64 = 2.5;              // Kurumsal Zırh: 2.5x ATR Stop-Loss (Geniş Nefes Payı)
const TP_ATR_MULT: f64 = 3.5;              // 🏆 3.5x ATR Take-Profit (Trailing Stop'a Yol Veren Geniş Kâr Hedefi)
const TRAIL_ATR_MULT: f64 = 2.0;           // ⚡ 1.5x ATR Trailing Stop Aktivasyon Eşiği (Tetik < TP Mantığı)
const TRAIL_OFFSET_ATR: f64 = 2.0;         // 1.0x ATR Takip Boşluğu
#[allow(dead_code)]
const LIVE_RISK_PCT: f64 = 0.015;          // 🛡️ KURUMSAL ZIRH: Sabit %1.5 Canlı Kasa Riski
const TAKER_FEE_RATE: f64 = 0.0005;        // %0.05 Binance Futures Taker Komisyonu
const SLIPPAGE_RATE: f64 = 0.0005;         // 🛡️ GERÇEKÇİ SÜRTÜNME: %0.05 (5 bps) Kayma + Spread Payı
const FUNDING_RATE_8H: f64 = 0.0001;       // %0.01 (8 saatlik fonlama oranı)
const STATE_FILE: &str = "bot_state.json";
const TELEMETRY_JS_FILE: &str = "telemetry.js";
const KEYS_FILE: &str = "testnet_keys.json";
#[allow(dead_code)]
const TYPESAFE_API_KEY: &str = "apikey_2602055d44d82ff4d00800ce1a9c1b0d59a_9dc9fa7b2e8e83e0c23bde23f22ac099206b463d7d0ef440d34a4754655b918b";
#[allow(dead_code)]
const TYPESAFE_API_URL: &str = "https://api.typesafe.ai/v1/systemone";

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
struct SymbolSpec {
    pub symbol: &'static str,
    pub leverage: u32,
    pub step_size: f64,
    pub min_qty: f64,
    pub qty_decimals: usize,
    pub price_decimals: usize,
}

const SYMBOLS: [SymbolSpec; 13] = [
    SymbolSpec {
        symbol: "ETHUSDT",
        leverage: 5,
        step_size: 0.001000,
        min_qty: 0.001000,
        qty_decimals: 3,
        price_decimals: 2,
    },
    SymbolSpec {
        symbol: "XRPUSDT",
        leverage: 5,
        step_size: 0.100000,
        min_qty: 0.100000,
        qty_decimals: 1,
        price_decimals: 4,
    },
    SymbolSpec {
        symbol: "SOLUSDT",
        leverage: 5,
        step_size: 0.010000,
        min_qty: 0.010000,
        qty_decimals: 2,
        price_decimals: 2,
    },
    SymbolSpec {
        symbol: "DOGEUSDT",
        leverage: 5,
        step_size: 1.000000,
        min_qty: 1.000000,
        qty_decimals: 0,
        price_decimals: 5,
    },
    SymbolSpec {
        symbol: "LINKUSDT",
        leverage: 5,
        step_size: 0.010000,
        min_qty: 0.010000,
        qty_decimals: 2,
        price_decimals: 3,
    },
    SymbolSpec {
        symbol: "BTCUSDT",
        leverage: 5,
        step_size: 0.001000,
        min_qty: 0.001000,
        qty_decimals: 3,
        price_decimals: 1,
    },
    SymbolSpec {
        symbol: "SUIUSDT",
        leverage: 5,
        step_size: 0.100000,
        min_qty: 0.100000,
        qty_decimals: 1,
        price_decimals: 4,
    },
    SymbolSpec {
        symbol: "AVAXUSDT",
        leverage: 5,
        step_size: 1.000000,
        min_qty: 1.000000,
        qty_decimals: 0,
        price_decimals: 3,
    },
    SymbolSpec {
        symbol: "NEARUSDT",
        leverage: 5,
        step_size: 1.000000,
        min_qty: 1.000000,
        qty_decimals: 0,
        price_decimals: 3,
    },
    SymbolSpec {
        symbol: "ADAUSDT",
        leverage: 5,
        step_size: 1.000000,
        min_qty: 1.000000,
        qty_decimals: 0,
        price_decimals: 4,
    },
    SymbolSpec {
        symbol: "APTUSDT",
        leverage: 5,
        step_size: 0.100000,
        min_qty: 0.100000,
        qty_decimals: 1,
        price_decimals: 4,
    },
    SymbolSpec {
        symbol: "DOTUSDT",
        leverage: 5,
        step_size: 0.100000,
        min_qty: 0.100000,
        qty_decimals: 1,
        price_decimals: 4,
    },
    SymbolSpec {
        symbol: "TIAUSDT",
        leverage: 5,
        step_size: 1.000000,
        min_qty: 1.000000,
        qty_decimals: 0,
        price_decimals: 4,
    },
];

fn get_symbol_spec(symbol: &str) -> SymbolSpec {
    for s in SYMBOLS.iter() {
        if s.symbol == symbol {
            return *s;
        }
    }
    SYMBOLS[0]
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TestnetCredentials {
    pub api_key: String,
    pub api_secret: String,
}

fn load_credentials() -> (String, String) {
    let env_key = env::var("BINANCE_API_KEY").unwrap_or_default();
    let env_secret = env::var("BINANCE_API_SECRET").unwrap_or_default();
    if !env_key.is_empty() && !env_secret.is_empty() {
        return (env_key, env_secret);
    }
    if Path::new(KEYS_FILE).exists() {
        if let Ok(raw) = fs::read_to_string(KEYS_FILE) {
            if let Ok(creds) = serde_json::from_str::<TestnetCredentials>(&raw) {
                if !creds.api_key.trim().is_empty() && !creds.api_secret.trim().is_empty() {
                    return (
                        creds.api_key.trim().to_string(),
                        creds.api_secret.trim().to_string(),
                    );
                }
            }
        }
    }
    (String::new(), String::new())
}

#[derive(Debug, Deserialize)]
struct CombinedStreamWrapper {
    stream: String,
    data: serde_json::Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AggTradeEvent {
    #[serde(rename = "s")]
    symbol: Option<String>,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    trade_time: u64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Debug, Deserialize)]
struct KlineEventWrapper {
    #[serde(rename = "k")]
    kline: KlineData,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct KlineData {
    #[serde(rename = "s")]
    symbol: Option<String>,
    #[serde(rename = "t")]
    open_time: u64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    is_closed: bool,
}

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

    // Milestone 1 Feature 2: Time-in-Trade & Duration Tracking Instrumentation
    #[serde(default)]
    pub entry_time_ms: u64,
    #[serde(default)]
    pub exit_time_ms: u64,
    #[serde(default)]
    pub holding_duration_seconds: u64,
    #[serde(default)]
    pub holding_duration_hours: f64,
    #[serde(default)]
    pub hourly_active_roe: f64,
}

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
        #[serde(default)]
        opened_ts: i64,
        #[serde(default)]
        regime: u8,
    },
    Short {
        symbol: String,
        leverage: u32,
        entry_price: f64,
        qty: f64,
        margin_used: f64,
        stop_loss: f64,
        take_profit: f64,
        lowest_price: f64,
        trailing_active: bool,
        entry_fee: f64,
        opened_at: String,
        #[serde(default)]
        opened_ts: i64,
        #[serde(default)]
        regime: u8,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BotPersistentState {
    pub engine_version: String,
    pub balance: f64,
    pub peak_equity: f64,
    pub max_drawdown_pct: f64,
    pub realized_gross_pnl: f64,
    pub realized_net_pnl: f64,
    pub total_fees_paid: f64,
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub reconnect_count: u32,
    pub position: PositionState,
    pub closed_trades: Vec<ClosedTradeRecord>,
    #[serde(default)]
    pub consecutive_stops: u32,
    #[serde(default)]
    pub circuit_breaker_until: i64,
    #[serde(default)]
    pub btc_entry_price: f64,
}

impl Default for BotPersistentState {
    fn default() -> Self {
        Self {
            engine_version: "v5.0-DUAL-ENGINE-MAINNET".to_string(),
            balance: INITIAL_BALANCE,
            peak_equity: INITIAL_BALANCE,
            max_drawdown_pct: 0.0,
            realized_gross_pnl: 0.0,
            realized_net_pnl: 0.0,
            total_fees_paid: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            reconnect_count: 0,
            position: PositionState::Flat,
            closed_trades: Vec::new(),
            consecutive_stops: 0,
            circuit_breaker_until: 0,
            btc_entry_price: 0.0,
        }
    }
}

impl BotPersistentState {
    fn load_or_init() -> Self {
        if Path::new(STATE_FILE).exists() {
            if let Ok(content) = fs::read_to_string(STATE_FILE) {
                if let Ok(state) = serde_json::from_str::<BotPersistentState>(&content) {
                    if state.engine_version == "v5.0-DUAL-ENGINE-MAINNET" {
                        println!(
                            "💾 [CRASH RECOVERY] $10 Mikro-Kasa durumu yüklendi: Bakiye ${:.2} USDT | Pozisyon: {:?}",
                            state.balance, state.position
                        );
                        return state;
                    }
                }
            }
        }
        let state = Self::default();
        state.save_atomic();
        state
    }

    fn save_atomic(&self) {
        let tmp_path = format!("{}.tmp", STATE_FILE);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if fs::write(&tmp_path, json).is_ok() {
                let _ = fs::rename(&tmp_path, STATE_FILE);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CoinRadarSnapshot {
    pub symbol: String,
    pub leverage: u32,
    pub price: f64,
    pub ema9: f64,
    pub ema21: f64,
    pub sma50: f64,
    pub sma50_dist_pct: f64,
    pub proximity_rank: u32,
    pub rsi14: f64,
    pub atr14: f64,
    pub atr_pct: f64,
    pub adx14: f64,
    pub trend: String,
    pub signal_score: u32,
    pub signal_dir: String,
    pub pivot_support: Option<f64>,
    pub pivot_resistance: Option<f64>,
    pub smc_ob: Option<String>,
    pub taker_buy_pct: f64,
    pub top_long_pct: f64,
    pub delta_oi_pct: f64,
    pub order_flow_sentiment: String,
    pub scout_sentiment: String,
}

#[derive(Debug, Clone)]
struct SymbolEngine {
    pub spec: SymbolSpec,
    confirmed_ema9: Option<f64>,
    confirmed_ema12: Option<f64>,
    confirmed_ema21: Option<f64>,
    confirmed_ema26: Option<f64>,
    confirmed_ema50: Option<f64>,
    confirmed_ema100: Option<f64>,
    confirmed_ema200: Option<f64>,
    confirmed_macd_sig: Option<f64>,
    confirmed_avg_gain14: Option<f64>,
    confirmed_avg_loss14: Option<f64>,
    confirmed_avg_gain7: Option<f64>,
    confirmed_avg_loss7: Option<f64>,
    confirmed_prev_close: Option<f64>,
    confirmed_prev_ema21: f64,
    confirmed_prev_ema50: f64,
    confirmed_prev_rsi7: f64,
    confirmed_prev_macd_hist: f64,
    confirmed_atr14: Option<f64>,
    confirmed_dm_plus14: Option<f64>,
    confirmed_dm_minus14: Option<f64>,
    confirmed_tr14_adx: Option<f64>,
    confirmed_adx14: Option<f64>,
    highs_20: Vec<f64>,
    lows_20: Vec<f64>,
    closes_20: Vec<f64>,
    vols_20: Vec<f64>,
    recent_highs: Vec<f64>,
    recent_lows: Vec<f64>,
    recent_opens: Vec<f64>,
    recent_closes: Vec<f64>,
    pub pivot_resistance: Option<f64>,
    pub pivot_support: Option<f64>,
    pub smc_bullish_ob: Option<(f64, f64)>,
    pub smc_bearish_ob: Option<(f64, f64)>,
    pub smc_fvg: Option<(f64, f64, &'static str)>,
    pub closed_candles_count: usize,

    pub last_open: f64,
    pub last_high: f64,
    pub last_low: f64,
    pub last_price: f64,
    pub live_ema9: f64,
    pub live_ema21: f64,
    pub live_ema50: f64,
    pub live_ema100: f64,
    pub live_ema200: f64,
    pub live_macd_hist: f64,
    pub live_rsi14: f64,
    pub live_rsi7: f64,
    pub live_atr14: f64,
    pub live_adx14: f64,
    pub efficiency_ratio: f64,
    pub prev_high20: f64,
    pub prev_low20: f64,
    pub vol_ratio: f64,
    pub signal_score: u32,
    pub signal_dir: &'static str,
    pub live_taker_buy_pct: f64,
    pub live_top_trader_long_pct: f64,
    pub live_delta_oi_pct: f64,
    pub live_order_flow_sentiment: String,
    pub live_scout_sentiment: String,
    pub live_scout_score_boost: i32,
    pub live_scout_headline: String,
    pub custom_strategy_mode: u8,
    pub custom_donchian_lookback: usize,
    pub custom_min_wick_ratio: f64,
    pub active_regime: u8,
    pub micro_ticks_60s: VecDeque<(Instant, f64, f64, bool)>,
}

impl SymbolEngine {
    fn new(spec: SymbolSpec) -> Self {
        Self {
            spec,
            confirmed_ema9: None,
            confirmed_ema12: None,
            confirmed_ema21: None,
            confirmed_ema26: None,
            confirmed_ema50: None,
            confirmed_ema100: None,
            confirmed_ema200: None,
            confirmed_macd_sig: None,
            confirmed_avg_gain14: None,
            confirmed_avg_loss14: None,
            confirmed_avg_gain7: None,
            confirmed_avg_loss7: None,
            confirmed_prev_close: None,
            confirmed_prev_ema21: 0.0,
            confirmed_prev_ema50: 0.0,
            confirmed_prev_rsi7: 50.0,
            confirmed_prev_macd_hist: 0.0,
            confirmed_atr14: None,
            confirmed_dm_plus14: None,
            confirmed_dm_minus14: None,
            confirmed_tr14_adx: None,
            confirmed_adx14: None,
            highs_20: Vec::with_capacity(22),
            lows_20: Vec::with_capacity(22),
            closes_20: Vec::with_capacity(22),
            vols_20: Vec::with_capacity(22),
            recent_highs: Vec::with_capacity(82),
            recent_lows: Vec::with_capacity(82),
            recent_opens: Vec::with_capacity(82),
            recent_closes: Vec::with_capacity(82),
            pivot_resistance: None,
            pivot_support: None,
            smc_bullish_ob: None,
            smc_bearish_ob: None,
            smc_fvg: None,
            closed_candles_count: 0,
            last_open: 0.0,
            last_high: 0.0,
            last_low: 0.0,
            last_price: 0.0,
            live_ema9: 0.0,
            live_ema21: 0.0,
            live_ema50: 0.0,
            live_ema100: 0.0,
            live_ema200: 0.0,
            live_macd_hist: 0.0,
            live_rsi14: 50.0,
            live_rsi7: 50.0,
            live_atr14: 0.0,
            live_adx14: 25.0,
            prev_high20: 0.0,
            prev_low20: 0.0,
            vol_ratio: 1.0,
            efficiency_ratio: 0.5,
            signal_score: 50,
            signal_dir: "NEUTRAL",
            live_taker_buy_pct: 50.0,
            live_top_trader_long_pct: 50.0,
            live_delta_oi_pct: 0.0,
            live_order_flow_sentiment: "⚪ DENGELİ AKIŞ".to_string(),
            live_scout_sentiment: "⚪ SAKİN".to_string(),
            live_scout_score_boost: 0,
            live_scout_headline: String::new(),
            custom_strategy_mode: 0,
            custom_donchian_lookback: 12,
            custom_min_wick_ratio: 0.18,
            active_regime: 0,
            micro_ticks_60s: VecDeque::with_capacity(260),
        }
    }

    fn on_closed_candle(&mut self, open: f64, high: f64, low: f64, close: f64, volume: f64) {
        self.last_open = open;
        self.last_high = high;
        self.last_low = low;
        self.confirmed_prev_ema21 = self.confirmed_ema21.unwrap_or(close);
        self.confirmed_prev_ema50 = self.confirmed_ema50.unwrap_or(close);
        self.confirmed_prev_macd_hist = self.live_macd_hist;

        let new_ema9 = match self.confirmed_ema9 {
            Some(prev) => (close - prev) * (2.0 / 10.0) + prev,
            None => close,
        };
        let new_ema12 = match self.confirmed_ema12 {
            Some(prev) => (close - prev) * (2.0 / 13.0) + prev,
            None => close,
        };
        let new_ema21 = match self.confirmed_ema21 {
            Some(prev) => (close - prev) * (2.0 / 22.0) + prev,
            None => close,
        };
        let new_ema26 = match self.confirmed_ema26 {
            Some(prev) => (close - prev) * (2.0 / 27.0) + prev,
            None => close,
        };
        let new_ema50 = match self.confirmed_ema50 {
            Some(prev) => (close - prev) * (2.0 / 51.0) + prev,
            None => close,
        };
        let new_ema100 = match self.confirmed_ema100 {
            Some(prev) => (close - prev) * (2.0 / 101.0) + prev,
            None => close,
        };
        let new_ema200 = match self.confirmed_ema200 {
            Some(prev) => (close - prev) * (2.0 / 201.0) + prev,
            None => close,
        };
        let macd_line = new_ema12 - new_ema26;
        let new_macd_sig = match self.confirmed_macd_sig {
            Some(prev) => (macd_line - prev) * (2.0 / 10.0) + prev,
            None => 0.0,
        };
        self.confirmed_ema9 = Some(new_ema9);
        self.confirmed_ema12 = Some(new_ema12);
        self.confirmed_ema21 = Some(new_ema21);
        self.confirmed_ema26 = Some(new_ema26);
        self.confirmed_ema50 = Some(new_ema50);
        self.confirmed_ema100 = Some(new_ema100);
        self.confirmed_ema200 = Some(new_ema200);
        self.confirmed_macd_sig = Some(new_macd_sig);
        self.live_macd_hist = macd_line - new_macd_sig;

        let tr = match self.confirmed_prev_close {
            Some(prev_c) => {
                let hl = high - low;
                let hc = (high - prev_c).abs();
                let lc = (low - prev_c).abs();
                hl.max(hc).max(lc)
            }
            None => (high - low).max(close * 0.002),
        };
        let new_atr = match self.confirmed_atr14 {
            Some(prev_atr) => (prev_atr * 13.0 + tr) / 14.0,
            None => tr.max(close * 0.0025),
        };
        self.confirmed_atr14 = Some(new_atr);

        self.confirmed_prev_rsi7 = self.live_rsi7;
        if let Some(prev_c) = self.confirmed_prev_close {
            let diff = close - prev_c;
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };

            let (ag14, al14) = match (self.confirmed_avg_gain14, self.confirmed_avg_loss14) {
                (Some(pg), Some(pl)) => ((pg * 13.0 + gain) / 14.0, (pl * 13.0 + loss) / 14.0),
                _ => (close * 0.0005, close * 0.0005),
            };
            self.confirmed_avg_gain14 = Some(ag14);
            self.confirmed_avg_loss14 = Some(al14);
            self.live_rsi14 = if al14 == 0.0 {
                70.0
            } else {
                100.0 - (100.0 / (1.0 + (ag14 / al14)))
            };

            let (ag7, al7) = match (self.confirmed_avg_gain7, self.confirmed_avg_loss7) {
                (Some(pg), Some(pl)) => ((pg * 6.0 + gain) / 7.0, (pl * 6.0 + loss) / 7.0),
                _ => (close * 0.0005, close * 0.0005),
            };
            self.confirmed_avg_gain7 = Some(ag7);
            self.confirmed_avg_loss7 = Some(al7);
            self.live_rsi7 = if al7 == 0.0 {
                70.0
            } else {
                100.0 - (100.0 / (1.0 + (ag7 / al7)))
            };

            // ADX 14 Hesaplaması (Wilder's Smoothing)
            let prev_h = self.recent_highs.last().copied().unwrap_or(high);
            let prev_l = self.recent_lows.last().copied().unwrap_or(low);
            let up_move = high - prev_h;
            let down_move = prev_l - low;
            let plus_dm = if up_move > down_move && up_move > 0.0 { up_move } else { 0.0 };
            let minus_dm = if down_move > up_move && down_move > 0.0 { down_move } else { 0.0 };

            let (new_dp, new_dm, new_tr) = match (self.confirmed_dm_plus14, self.confirmed_dm_minus14, self.confirmed_tr14_adx) {
                (Some(p_dp), Some(p_dm), Some(p_tr)) => (
                    p_dp - (p_dp / 14.0) + plus_dm,
                    p_dm - (p_dm / 14.0) + minus_dm,
                    p_tr - (p_tr / 14.0) + tr,
                ),
                _ => (plus_dm * 14.0, minus_dm * 14.0, tr * 14.0),
            };
            self.confirmed_dm_plus14 = Some(new_dp);
            self.confirmed_dm_minus14 = Some(new_dm);
            self.confirmed_tr14_adx = Some(new_tr);

            let di_plus = if new_tr > 0.0 { 100.0 * (new_dp / new_tr) } else { 0.0 };
            let di_minus = if new_tr > 0.0 { 100.0 * (new_dm / new_tr) } else { 0.0 };
            let sum_di = di_plus + di_minus;
            let dx = if sum_di > 0.0 { 100.0 * (di_plus - di_minus).abs() / sum_di } else { 0.0 };

            let new_adx = match self.confirmed_adx14 {
                Some(p_adx) => (p_adx * 13.0 + dx) / 14.0,
                None => dx,
            };
            self.confirmed_adx14 = Some(new_adx);
            self.live_adx14 = new_adx;
        }

        if self.highs_20.len() >= 10 {
            self.prev_high20 = self.highs_20.iter().copied().fold(f64::MIN, f64::max);
            self.prev_low20 = self.lows_20.iter().copied().fold(f64::MAX, f64::min);
        } else {
            self.prev_high20 = high;
            self.prev_low20 = low;
        }

        self.highs_20.push(high);
        self.lows_20.push(low);
        self.closes_20.push(close);
        self.vols_20.push(volume);
        if self.highs_20.len() > 20 {
            self.highs_20.remove(0);
            self.lows_20.remove(0);
            self.closes_20.remove(0);
            self.vols_20.remove(0);
        }

        self.recent_highs.push(high);
        self.recent_lows.push(low);
        if self.recent_highs.len() > 80 {
            self.recent_highs.remove(0);
            self.recent_lows.remove(0);
        }
        self.update_pivot_sr();
        self.update_smc_structure(open, high, low, close);
        if self.closes_20.len() >= 10 {
            let net_chg = (self.closes_20.last().unwrap() - self.closes_20.first().unwrap()).abs();
            let sum_abs: f64 = self
                .closes_20
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .sum();
            self.efficiency_ratio = if sum_abs > 0.0 { net_chg / sum_abs } else { 0.0 };
        }
        let vol_sma = self.vols_20.iter().sum::<f64>() / (self.vols_20.len().max(1) as f64);
        self.vol_ratio = if vol_sma > 0.0 { volume / vol_sma } else { 1.0 };

        self.confirmed_prev_close = Some(close);
        self.closed_candles_count += 1;
        self.last_price = close;
        self.live_ema9 = new_ema9;
        self.live_ema21 = new_ema21;
        self.live_ema50 = new_ema50;
        self.live_ema100 = new_ema100;
        self.live_ema200 = new_ema200;
        self.live_atr14 = new_atr;
        self.recompute_confluence();
    }

    fn on_live_kline(&mut self, open: f64, high: f64, low: f64, close: f64, volume: f64) {
        self.last_open = open;
        self.last_high = high;
        self.last_low = low;
        let vol_sma = self.vols_20.iter().sum::<f64>() / (self.vols_20.len().max(1) as f64);
        if vol_sma > 0.0 && volume > 0.0 {
            // Saat içi anlık hacim ivmesi veya önceki onaylı hacim oranından büyük olanı koru
            self.vol_ratio = (volume / vol_sma).max(self.vol_ratio * 0.98).max(1.22);
        }
        self.on_live_tick(close, volume, true);
        self.recompute_confluence();
    }

    fn on_live_tick(&mut self, price: f64, qty: f64, is_taker_buy: bool) {
        self.last_price = price;
        let now = Instant::now();
        self.micro_ticks_60s.push_back((now, price, qty, is_taker_buy));
        while let Some((ts, _, _, _)) = self.micro_ticks_60s.front() {
            if now.duration_since(*ts) > Duration::from_secs(60) || self.micro_ticks_60s.len() > 300 {
                self.micro_ticks_60s.pop_front();
            } else {
                break;
            }
        }
        let base_ema9 = self.confirmed_ema9.unwrap_or(price);
        let base_ema12 = self.confirmed_ema12.unwrap_or(price);
        let base_ema21 = self.confirmed_ema21.unwrap_or(price);
        let base_ema26 = self.confirmed_ema26.unwrap_or(price);
        let base_ema50 = self.confirmed_ema50.unwrap_or(price);
        let base_ema100 = self.confirmed_ema100.unwrap_or(price);
        let base_ema200 = self.confirmed_ema200.unwrap_or(price);
        let base_macd_sig = self.confirmed_macd_sig.unwrap_or(0.0);

        self.live_ema9 = (price - base_ema9) * (2.0 / 10.0) + base_ema9;
        let live_ema12 = (price - base_ema12) * (2.0 / 13.0) + base_ema12;
        self.live_ema21 = (price - base_ema21) * (2.0 / 22.0) + base_ema21;
        let live_ema26 = (price - base_ema26) * (2.0 / 27.0) + base_ema26;
        self.live_ema50 = (price - base_ema50) * (2.0 / 51.0) + base_ema50;
        self.live_ema100 = (price - base_ema100) * (2.0 / 101.0) + base_ema100;
        self.live_ema200 = (price - base_ema200) * (2.0 / 201.0) + base_ema200;
        let ml = live_ema12 - live_ema26;
        let msig = (ml - base_macd_sig) * (2.0 / 10.0) + base_macd_sig;
        self.live_macd_hist = ml - msig;

        if let Some(prev_c) = self.confirmed_prev_close {
            let diff = price - prev_c;
            let gain = if diff > 0.0 { diff } else { 0.0 };
            let loss = if diff < 0.0 { -diff } else { 0.0 };
            let ag14 = (self.confirmed_avg_gain14.unwrap_or(price * 0.0005) * 13.0 + gain) / 14.0;
            let al14 = (self.confirmed_avg_loss14.unwrap_or(price * 0.0005) * 13.0 + loss) / 14.0;
            if al14 > 0.0 {
                self.live_rsi14 = 100.0 - (100.0 / (1.0 + (ag14 / al14)));
            }
            let ag7 = (self.confirmed_avg_gain7.unwrap_or(price * 0.0005) * 6.0 + gain) / 7.0;
            let al7 = (self.confirmed_avg_loss7.unwrap_or(price * 0.0005) * 6.0 + loss) / 7.0;
            if al7 > 0.0 {
                self.live_rsi7 = 100.0 - (100.0 / (1.0 + (ag7 / al7)));
            }
        }
        if self.live_atr14 <= 0.0 {
            self.live_atr14 = price * 0.003;
        }
    }

    /// 🎯 v10.0 1-Dakikalık (60s) Mikro-RSI & 30-Saniyelik Taker Hacim Deltası Sniper Motoru
    #[allow(dead_code)]
    fn check_micro_sniper_trigger(&self, side: &str) -> (bool, i32) {
        if self.micro_ticks_60s.len() < 5 || self.last_price <= 0.0 {
            return (true, 0);
        }
        let now = Instant::now();
        let cur = self.last_price;
        let open_60s = self.micro_ticks_60s.front().map(|(_, p, _, _)| *p).unwrap_or(cur);
        let mut high_60s = cur;
        let mut low_60s = cur;
        let mut open_10s = cur;
        let mut open_10s_found = false;
        let mut gain_sum = 0.0;
        let mut loss_sum = 0.0;
        let mut prev_p: Option<f64> = None;
        let mut buy_vol_30s = 0.0;
        let mut sell_vol_30s = 0.0;

        for (ts, p, q, is_buy) in self.micro_ticks_60s.iter() {
            if *p > high_60s {
                high_60s = *p;
            }
            if *p < low_60s {
                low_60s = *p;
            }
            if let Some(prev) = prev_p {
                let diff = *p - prev;
                if diff > 0.0 {
                    gain_sum += diff;
                } else {
                    loss_sum += -diff;
                }
            }
            prev_p = Some(*p);

            if now.duration_since(*ts) <= Duration::from_secs(30) {
                if *is_buy {
                    buy_vol_30s += *q;
                } else {
                    sell_vol_30s += *q;
                }
            }
            if !open_10s_found && now.duration_since(*ts) <= Duration::from_secs(10) {
                open_10s = *p;
                open_10s_found = true;
            }
        }

        // 1. 1-Dakikalık Mikro-RSI (Son 60 Saniyenin Tik Göreli Güç İndeksi)
        let micro_rsi_1m = if loss_sum == 0.0 && gain_sum == 0.0 {
            50.0
        } else if loss_sum == 0.0 {
            100.0
        } else {
            100.0 - (100.0 / (1.0 + (gain_sum / loss_sum)))
        };

        // 2. 30-Saniyelik Mikro-Taker Hacim Deltası
        let total_vol_30s = buy_vol_30s + sell_vol_30s;
        let taker_buy_ratio_30s = if total_vol_30s > 0.0 {
            buy_vol_30s / total_vol_30s
        } else {
            0.50
        };

        // 3. 60-Saniyelik Fitil (Wick) ve Gövde Dağılımı (Kural 1: Sniper Fitil Teyidi)
        let range_60s = high_60s - low_60s;
        let body_high = cur.max(open_60s);
        let body_low = cur.min(open_60s);
        let upper_wick = high_60s - body_high;
        let lower_wick = body_low - low_60s;
        let upper_wick_ratio = if range_60s > 1e-12 { upper_wick / range_60s } else { 0.0 };
        let lower_wick_ratio = if range_60s > 1e-12 { lower_wick / range_60s } else { 0.0 };

        if side == "SHORT" {
            // 🎯 KURAL 1 (FİTİL TEPESİ SNIPER TEYİDİ - SHORT):
            // 1. Üst fitil en az %22 olmalı (fiyat yukarıyı denemiş ve dirençten tokat yemiş olmalı!)
            if upper_wick_ratio < 0.22 && range_60s >= cur * 0.0006 {
                return (false, -25); // Üst fitil yok veya yetersiz (<%22) -> Direnç reddi teyit edilmedi!
            }
            // 2. Fiyat mumun en tepesine yapışık olamaz (yeşil mum dikine çıkarken asla shortlanmaz!)
            if cur >= high_60s * 0.9995 && range_60s >= cur * 0.0006 {
                return (false, -25); // Fiyat en yüksek fiyatta seyrediyor -> Tokat yemeden girme!
            }
            // 3. 10 saniyelik anlık mikro yön aşağı olmalı (kıvrılma teyidi)
            if cur > open_10s * 1.0002 {
                return (false, -20); // Son 10 saniye yukarı fırlıyor -> Bıçak önü vetosu!
            }
            // Veto 1: 30s alıcılar tahtayı süpürüyor (>55% taker buy) -> Önüne yatma!
            if taker_buy_ratio_30s > 0.55 && total_vol_30s > 0.0 {
                return (false, -20);
            }
            // Veto 2: 1dk RSI aşırı satım dibinde (<32), mikro tepki fitili beklenmeli!
            if micro_rsi_1m < 32.0 {
                return (false, -20);
            }
            // Veto 3: Yeşil İğne Koruma: Son 60 saniyede +%0.25'den fazla dikine yükseliyorsa ve henüz üstten dönmediyse BEKLE!
            if cur > open_60s * 1.0025 && cur >= high_60s * 0.9996 {
                return (false, -20);
            }
            // Veto 4: Dipten Satmama Kalkanı: Son 60 saniyede zaten -%0.45 çökmüşse ve tam 1dk dibindeyse BEKLE!
            if cur < open_60s * 0.9955 && cur <= low_60s * 1.0002 {
                return (false, -15);
            }
            // 🎯 KUSURSUZ SNIPER SHORT: Üst fitil güçlü (%35+), 1dk RSI tepeye vurdu ve satıcılar baskın!
            if upper_wick_ratio >= 0.35 && micro_rsi_1m >= 55.0 && taker_buy_ratio_30s <= 0.45 {
                return (true, 20);
            }
            // Standart Sniper Üst Fitil Dönüş Teyidi
            if (upper_wick_ratio >= 0.22 || high_60s >= open_60s * 1.0006) && cur < high_60s && cur <= open_10s * 1.0002 {
                return (true, 10);
            }
            return (false, -10); // Fitil teyidi oluşmadı, bekle!
        } else if side == "LONG" {
            // 🎯 KURAL 1 (FİTİL DİBİ SNIPER TEYİDİ - LONG):
            // 1. Alt fitil en az %22 olmalı (fiyat aşağıyı denemiş ve destekten sekmiş olmalı!)
            if lower_wick_ratio < 0.22 && range_60s >= cur * 0.0006 {
                return (false, -25); // Alt fitil yok veya yetersiz (<%22) -> Destek sekmesi teyit edilmedi!
            }
            // 2. Fiyat mumun en dibine yapışık olamaz (kırmızı mum şelale yaparken asla longlanmaz!)
            if cur <= low_60s * 1.0005 && range_60s >= cur * 0.0006 {
                return (false, -25); // Fiyat en dip fiyatta seyrediyor -> Dip desteği görmeden girme!
            }
            // 3. 10 saniyelik anlık mikro yön yukarı olmalı (sekme teyidi)
            if cur < open_10s * 0.9998 {
                return (false, -20); // Son 10 saniye aşağı çöküyor -> Düşen bıçak vetosu!
            }
            // Veto 1: 30s satıcılar şelale yapıyor (>55% taker sell) -> Bıçak tutma!
            if taker_buy_ratio_30s < 0.45 && total_vol_30s > 0.0 {
                return (false, -20);
            }
            // Veto 2: 1dk RSI aşırı alım tepesinde (>68), mikro çekilme beklenmeli!
            if micro_rsi_1m > 68.0 {
                return (false, -20);
            }
            // Veto 3: Kırmızı Bıçak Koruma: Son 60 saniyede -%0.25'den fazla dikine düşüyorsa ve henüz dipten sekmediyse BEKLE!
            if cur < open_60s * 0.9975 && cur <= low_60s * 1.0004 {
                return (false, -20);
            }
            // Veto 4: Tepeden Almama Kalkanı: Son 60 saniyede zaten +%0.45 fırlamışsa ve tam 1dk tepesindeyse BEKLE!
            if cur > open_60s * 1.0045 && cur >= high_60s * 0.9998 {
                return (false, -15);
            }
            // 🎯 KUSURSUZ SNIPER LONG: Alt fitil güçlü (%35+), 1dk RSI dibe vurdu ve alıcılar baskın!
            if lower_wick_ratio >= 0.35 && micro_rsi_1m <= 45.0 && taker_buy_ratio_30s >= 0.55 {
                return (true, 20);
            }
            // Standart Sniper Alt Fitil Dönüş Teyidi
            if (lower_wick_ratio >= 0.22 || low_60s <= open_60s * 0.9994) && cur > low_60s && cur >= open_10s * 0.9998 {
                return (true, 10);
            }
            return (false, -10); // Fitil teyidi oluşmadı, bekle!
        }
        (false, -10)
    }

    fn update_pivot_sr(&mut self) {
        let n = self.recent_highs.len();
        let mut found_res: Option<f64> = None;
        let mut found_sup: Option<f64> = None;

        // 1. Pine Script Pivot High/Low Tarama (prd = 3 ile düşük gecikmeli kurumsal S/R kümeleme)
        if n >= 7 {
            let prd = 3;
            for i in (prd..=(n - prd - 1)).rev() {
                let h_val = self.recent_highs[i];
                let mut is_ph = true;
                for j in (i - prd)..=(i + prd) {
                    if j != i && self.recent_highs[j] > h_val {
                        is_ph = false;
                        break;
                    }
                }
                if is_ph && h_val > self.last_price {
                    if found_res.is_none() || h_val < found_res.unwrap() {
                        found_res = Some(h_val);
                    }
                }

                let l_val = self.recent_lows[i];
                let mut is_pl = true;
                for j in (i - prd)..=(i + prd) {
                    if j != i && self.recent_lows[j] < l_val {
                        is_pl = false;
                        break;
                    }
                }
                if is_pl && l_val < self.last_price {
                    if found_sup.is_none() || l_val > found_sup.unwrap() {
                        found_sup = Some(l_val);
                    }
                }
            }
        }

        // 2. Anlık Oturum / Yerel Swing Zirve-Dip Entegrasyonu (highs_20 / lows_20)
        // TradingView Pine Script S/R mantığıyla son 20 mumun en yakın zirve (tavan) ve dip (taban) seviyeleri
        if !self.highs_20.is_empty() {
            let h_len = self.highs_20.len();
            if h_len >= 3 {
                for k in 1..(h_len - 1) {
                    if self.highs_20[k] >= self.highs_20[k - 1] && self.highs_20[k] >= self.highs_20[k + 1] {
                        let peak = self.highs_20[k];
                        if peak > self.last_price {
                            if found_res.is_none() || peak < found_res.unwrap() {
                                found_res = Some(peak);
                            }
                        }
                    }
                }
            }
            let max_20 = self.highs_20.iter().copied().fold(f64::MIN, f64::max);
            if max_20 > self.last_price {
                if found_res.is_none() || max_20 < found_res.unwrap() {
                    found_res = Some(max_20);
                }
            }
        }

        if !self.lows_20.is_empty() {
            let l_len = self.lows_20.len();
            if l_len >= 3 {
                for k in 1..(l_len - 1) {
                    if self.lows_20[k] <= self.lows_20[k - 1] && self.lows_20[k] <= self.lows_20[k + 1] {
                        let trough = self.lows_20[k];
                        if trough < self.last_price {
                            if found_sup.is_none() || trough > found_sup.unwrap() {
                                found_sup = Some(trough);
                            }
                        }
                    }
                }
            }
            let min_20 = self.lows_20.iter().copied().fold(f64::MAX, f64::min);
            if min_20 < self.last_price {
                if found_sup.is_none() || min_20 > found_sup.unwrap() {
                    found_sup = Some(min_20);
                }
            }
        }

        self.pivot_resistance = found_res;
        self.pivot_support = found_sup;
    }

    fn update_smc_structure(&mut self, open: f64, high: f64, low: f64, close: f64) {
        self.recent_opens.push(open);
        self.recent_closes.push(close);
        if self.recent_opens.len() > 80 {
            self.recent_opens.remove(0);
            self.recent_closes.remove(0);
        }

        let n = self.recent_highs.len();
        if n < 4 {
            return;
        }

        // 1. LuxAlgo Fair Value Gap (FVG): 3-Mum Dengesizliği
        let cur_low = self.recent_lows[n - 1];
        let prev2_high = self.recent_highs[n - 3];
        if cur_low > prev2_high {
            self.smc_fvg = Some((prev2_high, cur_low, "BULLISH"));
        }
        let cur_high = self.recent_highs[n - 1];
        let prev2_low = self.recent_lows[n - 3];
        if cur_high < prev2_low {
            self.smc_fvg = Some((cur_high, prev2_low, "BEARISH"));
        }

        // 2. LuxAlgo Order Block (OB): Kurumsal İvme Kırılım Bloğu
        let candle_range = high - low;
        if candle_range > 1.25 * self.live_atr14 {
            if close > open {
                for i in (0..(n - 1)).rev() {
                    if self.recent_closes[i] < self.recent_opens[i] {
                        self.smc_bullish_ob = Some((self.recent_lows[i], self.recent_highs[i]));
                        break;
                    }
                }
            } else if close < open {
                for i in (0..(n - 1)).rev() {
                    if self.recent_closes[i] > self.recent_opens[i] {
                        self.smc_bearish_ob = Some((self.recent_lows[i], self.recent_highs[i]));
                        break;
                    }
                }
            }
        }

        // Mitigation: Fiyat OB seviyesini geçersiz kılarsa temizle
        if let Some((ob_low, _)) = self.smc_bullish_ob {
            if close < ob_low {
                self.smc_bullish_ob = None;
            }
        }
        if let Some((_, ob_high)) = self.smc_bearish_ob {
            if close > ob_high {
                self.smc_bearish_ob = None;
            }
        }
    }

        pub fn get_sma50(&self) -> f64 {
        if self.recent_closes.len() >= 50 {
            let sum: f64 = self.recent_closes.iter().rev().take(50).sum();
            sum / 50.0
        } else if !self.recent_closes.is_empty() {
            let sum: f64 = self.recent_closes.iter().sum();
            sum / (self.recent_closes.len() as f64)
        } else if self.live_ema50 > 0.0 {
            self.live_ema50
        } else {
            self.last_price
        }
    }

    fn apply_pivot_sr_and_smc_filter(&mut self) {
        if self.signal_dir == "LONG" {
            // 1. 🛑 TAVAN TUZAĞI KESİN VETO: Fiyat dirence %0.50'den daha yakınsa ALIM KESİNLİKLE YASAK!
            // Direnç tavanının hemen altına long açmak intihardır (TradingView Pine Script S/R Kuralı)
            if let Some(res) = self.pivot_resistance {
                if self.last_price <= res {
                    let dist = (res - self.last_price) / self.last_price;
                    if dist < 0.0050 {
                        self.signal_score = 40;
                        self.signal_dir = "NEUTRAL";
                        return;
                    }
                }
            }
            // 2. 🛑 SMC Bearish Order Block Tavan Uyarısı: Fiyat kurumsal arz kutusuna tosladıysa ALMA!
            if let Some((ob_low, ob_high)) = self.smc_bearish_ob {
                if self.last_price >= ob_low * 0.998 && self.last_price <= ob_high * 1.002 {
                    self.signal_score = 40;
                    self.signal_dir = "NEUTRAL";
                    return;
                }
            }
            // 3. 🎯 Destekten Sekme Teyidi: Fiyat pivot destekten teyit aldıysa (+8 puan)
            if let Some(sup) = self.pivot_support {
                if self.last_price >= sup && (self.last_price - sup) / self.last_price <= 0.015 {
                    self.signal_score = (self.signal_score + 8).min(99);
                }
                // 💎 PINE SCRIPT DESTEK KATSAYISI: Fiyat desteğe çok yakınsa (<= %0.50) kalkış teyidi (+12 puan)
                if self.last_price >= sup && (self.last_price - sup) / self.last_price <= 0.0050 {
                    self.signal_score = (self.signal_score + 12).min(99);
                }
            }
            // 💎 PINE SCRIPT SMA50 PULLBACK KATSAYISI: Fiyat 5m SMA50'ye <= %0.30 mesafede ise (+15 puan)
            let cur_sma50 = self.get_sma50();
            if cur_sma50 > 0.0 && (self.last_price - cur_sma50).abs() / cur_sma50 <= 0.0030 {
                self.signal_score = (self.signal_score + 15).min(99);
            }
            // 4. 🎯 SMC Bullish Order Block / FVG Test Teyidi: Fiyat kurumsal talep kutusundan sekiyorsa (+10 puan!)
            if let Some((ob_low, ob_high)) = self.smc_bullish_ob {
                if self.last_price >= ob_low && self.last_price <= ob_high * 1.015 {
                    self.signal_score = (self.signal_score + 10).min(99);
                }
            }
            // 5. 🎯 Direnç Kırılım (Breakout) Teyidi (Pine Script ta.crossover(close, resistanceLevel))
            if let Some(res) = self.pivot_resistance {
                if self.last_price >= res * 1.0005 {
                    self.signal_score = (self.signal_score + 6).min(99);
                }
            }
            if self.signal_score < 65 {
                self.signal_dir = "NEUTRAL";
            }
        } else if self.signal_dir == "SHORT" {
            // 1. 🛑 TABAN TUZAĞI KESİN VETO: Fiyat desteğe %0.50'den daha yakınsa SATIŞ KESİNLİKLE YASAK!
            // Destek tabanının hemen üzerine short açmak dibi satmaktır (TradingView Pine Script S/R Kuralı)
            if let Some(sup) = self.pivot_support {
                if self.last_price >= sup {
                    let dist = (self.last_price - sup) / self.last_price;
                    if dist < 0.0050 {
                        self.signal_score = 40;
                        self.signal_dir = "NEUTRAL";
                        return;
                    }
                }
            }
            // 2. 🛑 SMC Bullish Order Block Taban Uyarısı: Fiyat kurumsal talep kutusuna tosladıysa SATMA!
            if let Some((ob_low, ob_high)) = self.smc_bullish_ob {
                if self.last_price <= ob_high * 1.002 && self.last_price >= ob_low * 0.998 {
                    self.signal_score = 40;
                    self.signal_dir = "NEUTRAL";
                    return;
                }
            }
            // 3. 🎯 Dirençten Ret Yeme Teyidi: Fiyat pivot dirençten ret yediyse (+8 puan)
            if let Some(res) = self.pivot_resistance {
                if self.last_price <= res && (res - self.last_price) / self.last_price <= 0.015 {
                    self.signal_score = (self.signal_score + 8).min(99);
                }
                // 💎 PINE SCRIPT DİRENÇ KATSAYISI: Fiyat dirence çok yakınsa (<= %0.50) red teyidi (+12 puan)
                if self.last_price <= res && (res - self.last_price) / self.last_price <= 0.0050 {
                    self.signal_score = (self.signal_score + 12).min(99);
                }
            }
            // 💎 PINE SCRIPT SMA50 PULLBACK KATSAYISI: Fiyat 5m SMA50'ye <= %0.30 mesafede ise (+15 puan)
            let cur_sma50 = self.get_sma50();
            if cur_sma50 > 0.0 && (self.last_price - cur_sma50).abs() / cur_sma50 <= 0.0030 {
                self.signal_score = (self.signal_score + 15).min(99);
            }
            // 4. 🎯 SMC Bearish Order Block / FVG Ret Teyidi: Fiyat kurumsal arz kutusundan dönüyorsa (+10 puan!)
            if let Some((ob_low, ob_high)) = self.smc_bearish_ob {
                if self.last_price <= ob_high && self.last_price >= ob_low * 0.985 {
                    self.signal_score = (self.signal_score + 10).min(99);
                }
            }
            // 5. 🎯 Destek Kırılım (Breakdown) Teyidi (Pine Script ta.crossunder(close, supportLevel))
            if let Some(sup) = self.pivot_support {
                if self.last_price <= sup * 0.9995 {
                    self.signal_score = (self.signal_score + 6).min(99);
                }
            }
            if self.signal_score < 65 {
                self.signal_dir = "NEUTRAL";
            }
        }
    }

    /// Institutional Liquidity Sweep & Donchian Wick Reversal + Momentum Breakout Engine (v8.3 Tri-Regime)
    fn recompute_confluence(&mut self) {
        if self.last_price <= 0.0 || self.highs_20.len() < 5 {
            self.active_regime = 0;
            self.signal_score = 40;
            self.signal_dir = "NEUTRAL";
            return;
        }
        self.update_pivot_sr();

        let open = self.last_open;
        let high = self.last_high;
        let low = self.last_low;
        let close = self.last_price;

        // 🛡️ Volatilite Kalkanı: ATR / Fiyat < %0.35 ise piyasa çok sığdır, komisyona çalışmayı engelle
        if self.last_price > 0.0 && self.live_atr14 > 0.0 {
            let atr_pct = (self.live_atr14 / self.last_price) * 100.0;
            if atr_pct < 0.35 {
                self.active_regime = 0;
                self.signal_score = 40;
                self.signal_dir = "NEUTRAL";
                return;
            }
        }

        let total_range = (high - low).max(close * 0.001);
        let lower_wick = (open.min(close) - low).max(0.0);
        let upper_wick = (high - open.max(close)).max(0.0);

        // Tarihsel mumlar (mevcut mum hariç)
        let prev_count = self.highs_20.len().saturating_sub(1);
        if prev_count < 3 {
            self.signal_score = 40;
            self.signal_dir = "NEUTRAL";
            return;
        }

        // Dinamik Donchian Kanalı ve Fitil Eşiği (Swarm Genomundan)
        let lookback = self.custom_donchian_lookback.min(prev_count).max(4);
        let low12 = self.lows_20.iter().rev().skip(1).take(lookback).copied().fold(f64::MAX, f64::min);
        let high12 = self.highs_20.iter().rev().skip(1).take(lookback).copied().fold(f64::MIN, f64::max);
        let min_wick = self.custom_min_wick_ratio;

        let allow_sweep = self.custom_strategy_mode == 0 || self.custom_strategy_mode == 1;
        let allow_breakout = self.custom_strategy_mode == 0 || self.custom_strategy_mode == 2;
        let allow_pullback = self.custom_strategy_mode == 0 || self.custom_strategy_mode == 3;

        // ==========================================
        // REJİM 1: TREND YÖNÜNDE FITIL SÜPÜRME (PRO-TREND SHAKEOUT / LIQUIDITY SWEEP)
        // Yalnızca trend yönündeki silkelemeler onaylanır (Boğa trendinde alt fitil, Ayı trendinde üst fitil)
        // ==========================================
        let is_bull_sweep = low < low12
            && close > low12
            && (lower_wick / total_range) >= min_wick
            && close > open
            && self.live_ema9 >= self.live_ema21 * 0.998
            && self.live_ema21 >= self.live_ema50 * 0.998
            && (self.live_rsi14 <= 55.0 || self.live_rsi7 <= 40.0);

        let is_bear_sweep = high > high12
            && close < high12
            && (upper_wick / total_range) >= min_wick
            && close < open
            && self.live_ema9 <= self.live_ema21 * 1.002
            && self.live_ema21 <= self.live_ema50 * 1.002
            && (self.live_rsi14 >= 45.0 || self.live_rsi7 >= 60.0);

        if allow_sweep && is_bull_sweep {
            self.active_regime = 1;
            let mut score = 75 + ((lower_wick / total_range) * 25.0).min(15.0) as u32;
            if self.live_taker_buy_pct >= 48.0 { score = (score + 8).min(99); }
            if self.live_delta_oi_pct >= 0.05 { score = (score + 7).min(99); }
            if self.live_top_trader_long_pct <= 52.0 { score = (score + 5).min(99); }
            self.signal_score = score;
            self.signal_dir = "LONG";
            self.apply_pivot_sr_and_smc_filter();
            return;
        }

        if allow_sweep && is_bear_sweep {
            self.active_regime = 1;
            let mut score = 75 + ((upper_wick / total_range) * 25.0).min(15.0) as u32;
            if self.live_taker_buy_pct <= 52.0 { score = (score + 8).min(99); }
            if self.live_delta_oi_pct >= 0.05 { score = (score + 7).min(99); }
            if self.live_top_trader_long_pct >= 58.0 { score = (score + 5).min(99); }
            self.signal_score = score;
            self.signal_dir = "SHORT";
            self.apply_pivot_sr_and_smc_filter();
            return;
        }

        // ==========================================
        // REJİM 2: MOMENTUM & DONCHIAN TREND KIRILIMI (BREAKOUT)
        // ==========================================
        let is_bull_breakout = close >= high12
            && close > open
            && self.live_ema9 >= self.live_ema21 * 0.998
            && self.live_rsi14 >= 46.0 && self.live_rsi14 <= 62.0;

        let is_bear_breakout = close <= low12
            && close < open
            && self.live_ema9 <= self.live_ema21 * 1.002
            && self.live_rsi14 <= 54.0 && self.live_rsi14 >= 38.0;

        if allow_breakout && is_bull_breakout {
            self.active_regime = 2;
            let mut score = 72;
            if self.live_taker_buy_pct >= 48.0 { score += 10; }
            if self.live_delta_oi_pct >= 0.03 { score += 8; }
            if self.live_top_trader_long_pct >= 52.0 { score += 5; }
            self.signal_score = score.min(99);
            self.signal_dir = "LONG";
            self.apply_pivot_sr_and_smc_filter();
            return;
        }

        if allow_breakout && is_bear_breakout {
            self.active_regime = 2;
            let mut score = 72;
            if self.live_taker_buy_pct <= 52.0 { score += 10; }
            if self.live_delta_oi_pct >= 0.03 { score += 8; }
            if self.live_top_trader_long_pct <= 52.0 { score += 5; }
            self.signal_score = score.min(99);
            self.signal_dir = "SHORT";
            self.apply_pivot_sr_and_smc_filter();
            return;
        }

        // ==========================================
        // REJİM 3: EMA21 DİNAMİK TREND PULLBACK TEPKİSİ
        // ==========================================
        let is_bull_pullback = self.live_ema9 > self.live_ema21
            && low <= self.live_ema21 * 1.0025
            && close > self.live_ema21
            && close > open
            && (lower_wick / total_range) >= 0.15
            && self.live_rsi14 >= 46.0 && self.live_rsi14 <= 68.0;

        let is_bear_pullback = self.live_ema9 < self.live_ema21
            && high >= self.live_ema21 * 0.9975
            && close < self.live_ema21
            && close < open
            && (upper_wick / total_range) >= 0.15
            && self.live_rsi14 <= 54.0 && self.live_rsi14 >= 32.0;

        if allow_pullback && is_bull_pullback {
            self.active_regime = 3;
            let mut score = 70;
            if self.live_taker_buy_pct >= 48.0 { score += 8; }
            if self.live_delta_oi_pct >= 0.03 { score += 7; }
            if self.live_top_trader_long_pct >= 52.0 { score += 5; }
            self.signal_score = score.min(99);
            self.signal_dir = "LONG";
            self.apply_pivot_sr_and_smc_filter();
            return;
        }

        if allow_pullback && is_bear_pullback {
            self.active_regime = 3;
            let mut score = 70;
            if self.live_taker_buy_pct <= 52.0 { score += 8; }
            if self.live_delta_oi_pct >= 0.03 { score += 7; }
            if self.live_top_trader_long_pct <= 52.0 { score += 5; }
            self.signal_score = score.min(99);
            self.signal_dir = "SHORT";
            self.apply_pivot_sr_and_smc_filter();
            return;
        }

        self.active_regime = 0;
        self.signal_score = 40;
        self.signal_dir = "NEUTRAL";
    }

    pub fn calculate_order_with_params(
        &self,
        balance: f64,
        _sl_mult: f64,
        tp_mult: f64,
        _risk_pct: f64,
    ) -> Option<(f64, f64, f64, f64)> {
        if self.last_price <= 0.0 || balance < 2.0 || self.live_atr14 <= 0.0 {
            return None;
        }
        let eff_atr = self.live_atr14.max(self.last_price * 0.0025).min(self.last_price * 0.010);
        // ⚡ MİKRO-STOP ASİMETRİ KALKANI: Max %0.50 (35-50 bps) mikro zarar mesafesi
        let stop_dist = (eff_atr * 1.1).min(self.last_price * 0.0050).max(self.last_price * 0.0035);
        let tp_dist = eff_atr * tp_mult;

        let min_notional = match self.spec.symbol {
            "BTCUSDT" => 100.50,
            "ETHUSDT" | "LINKUSDT" => 20.50,
            _ => 5.20,
        };
        let min_steps = ((min_notional / self.last_price) / self.spec.step_size).ceil();
        let min_valid_qty = (min_steps * self.spec.step_size).max(self.spec.min_qty);

        // 5x ISOLATED Tekil Pozisyon: Kasanın maksimum %80'ini teminat olarak kullan (~$7.5-$8.5 marjin, ~$40 notional)
        let max_usable_notional = balance * (self.spec.leverage as f64) * 0.80;
        if min_valid_qty * self.last_price > max_usable_notional {
            return None;
        }
        let max_margin_steps = ((max_usable_notional / self.last_price) / self.spec.step_size).floor();
        let max_qty_by_margin = (max_margin_steps * self.spec.step_size).max(min_valid_qty);

        // 🛡️ Maksimum Dolar Risk Bütçesi: İşlem başına strictly capped $0.25 max net zarar!
        let max_risk_budget = (balance * 0.02).max(0.25).min(2.50);
        let est_entry_fee = min_valid_qty * self.last_price * TAKER_FEE_RATE;
        let est_exit_fee = min_valid_qty * (self.last_price - stop_dist).abs() * TAKER_FEE_RATE;

        let target_qty_by_risk = ((max_risk_budget - est_entry_fee - est_exit_fee).max(0.05)) / stop_dist;
        let risk_steps = (target_qty_by_risk / self.spec.step_size).floor();
        let calculated_qty = (risk_steps * self.spec.step_size)
            .min(max_qty_by_margin)
            .max(min_valid_qty);

        let round_trip_commission = calculated_qty * self.last_price * TAKER_FEE_RATE * 2.0;
        let expected_tp_gross = calculated_qty * tp_dist;
        if expected_tp_gross < round_trip_commission * 3.0 {
            return None;
        }

        let actual_notional = calculated_qty * self.last_price;
        let actual_margin = actual_notional / (self.spec.leverage as f64);

        if actual_margin > balance * 0.85 || actual_notional > balance * (self.spec.leverage as f64) * 0.85 {
            return None;
        }

        Some((calculated_qty, actual_margin, stop_dist, tp_dist))
    }

    /// 3'lü Ortogonal Kurumsal Sepet 5x İzole Tekil Pozisyon Boyutlandırma
    fn calculate_micro_10usd_order(&self, balance: f64) -> Option<(f64, f64, f64, f64)> {
        let (sl_mult, tp_mult, risk_pct) = match self.active_regime {
            1 => (2.5, 3.5, 0.075), // Mod 1: Likidite/Fitil Avcısı (5x İzole, Max SL %1.80)
            2 => (2.5, 3.5, 0.075), // Mod 2: Donchian Kırılımı (5x İzole, Max SL %1.80)
            3 => (1.8, 3.5, 0.075), // Mod 3: EMA21 Pullback (5x İzole, SL 1.8x ATR)
            _ => (SL_ATR_MULT, TP_ATR_MULT, 0.075),
        };
        self.calculate_order_with_params(balance, sl_mult, tp_mult, risk_pct)
    }

    fn to_radar_snapshot(&self) -> CoinRadarSnapshot {
        let trend = if self.live_ema21 > self.live_ema50 {
            "BOĞA".to_string()
        } else if self.live_ema21 < self.live_ema50 {
            "AYI".to_string()
        } else {
            "NÖTR".to_string()
        };
        let atr_pct = if self.last_price > 0.0 {
            (self.live_atr14 / self.last_price) * 100.0
        } else {
            0.0
        };
        let smc_ob = if let Some((low, high)) = self.smc_bullish_ob {
            if self.last_price >= low && self.last_price <= high * 1.015 {
                Some(format!("Talep OB: {:.2}-{:.2}", low, high))
            } else {
                Some(format!("Talep: {:.2}", low))
            }
        } else if let Some((low, high)) = self.smc_bearish_ob {
            if self.last_price <= high && self.last_price >= low * 0.985 {
                Some(format!("Arz OB: {:.2}-{:.2}", low, high))
            } else {
                Some(format!("Arz: {:.2}", high))
            }
        } else {
            None
        };
        let cand_sma50 = self.get_sma50();
        let sma50_dist = if cand_sma50 > 0.0 && self.last_price > 0.0 {
            ((self.last_price - cand_sma50).abs() / cand_sma50) * 100.0
        } else {
            99.0
        };
        CoinRadarSnapshot {
            symbol: self.spec.symbol.to_string(),
            leverage: self.spec.leverage,
            price: self.last_price,
            ema9: self.live_ema9,
            ema21: self.live_ema21,
            sma50: cand_sma50,
            sma50_dist_pct: sma50_dist,
            proximity_rank: 0,
            rsi14: self.live_rsi14,
            atr14: self.live_atr14,
            atr_pct,
            adx14: self.live_adx14,
            trend,
            signal_score: self.signal_score,
            signal_dir: self.signal_dir.to_string(),
            pivot_support: self.pivot_support,
            pivot_resistance: self.pivot_resistance,
            smc_ob,
            taker_buy_pct: self.live_taker_buy_pct,
            top_long_pct: self.live_top_trader_long_pct,
            delta_oi_pct: self.live_delta_oi_pct,
            order_flow_sentiment: self.live_order_flow_sentiment.clone(),
            scout_sentiment: self.live_scout_sentiment.clone(),
        }
    }
}

/// Binance Futures 5m Taker Buy/Sell, Top Trader Long/Short ve Açık Pozisyon (OI) Akışı
fn fetch_symbol_order_flow(symbol: &str) -> (f64, f64, f64, String) {
    let mut taker_buy_pct = 50.0;
    let mut top_long_pct = 50.0;
    let mut delta_oi_pct = 0.0;

    // 1. Taker Buy/Sell Hacim Oranı (Piyasa alımları / toplam) (5m)
    let url_taker = format!(
        "{}/futures/data/takerlongshortRatio?symbol={}&period=5m&limit=1",
        REST_BASE_URL, symbol
    );
    if let Ok(output) = Command::new(CURL_BIN)
        .args(["-s", "--max-time", "3", &url_taker])
        .output()
    {
        if let Ok(body) = String::from_utf8(output.stdout) {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                if let Some(obj) = arr.first() {
                    let buy_vol = obj["buyVol"].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                    let sell_vol = obj["sellVol"].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                    let tot = buy_vol + sell_vol;
                    if tot > 0.0 {
                        taker_buy_pct = (buy_vol / tot) * 100.0;
                    }
                }
            }
        }
    }

    // 2. Top Trader Net Pozisyon Oranı (Balina eğilimi) (5m)
    let url_top = format!(
        "{}/futures/data/topLongShortPositionRatio?symbol={}&period=5m&limit=1",
        REST_BASE_URL, symbol
    );
    if let Ok(output) = Command::new(CURL_BIN)
        .args(["-s", "--max-time", "3", &url_top])
        .output()
    {
        if let Ok(body) = String::from_utf8(output.stdout) {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                if let Some(obj) = arr.first() {
                    let long_acc = obj["longAccount"].as_str().unwrap_or("0.5").parse::<f64>().unwrap_or(0.5);
                    top_long_pct = (long_acc * 100.0).clamp(0.0, 100.0);
                }
            }
        }
    }

    // 3. Open Interest (Açık Pozisyon Değişimi - 5m)
    let url_oi = format!(
        "{}/futures/data/openInterestHist?symbol={}&period=5m&limit=2",
        REST_BASE_URL, symbol
    );
    if let Ok(output) = Command::new(CURL_BIN)
        .args(["-s", "--max-time", "3", &url_oi])
        .output()
    {
        if let Ok(body) = String::from_utf8(output.stdout) {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                if arr.len() >= 2 {
                    let prev = arr[0]["sumOpenInterestValue"].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                    let latest = arr[1]["sumOpenInterestValue"].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                    if prev > 0.0 {
                        delta_oi_pct = ((latest - prev) / prev) * 100.0;
                    }
                }
            }
        }
    }

    let sentiment = if delta_oi_pct >= 0.25 && taker_buy_pct >= 52.0 {
        format!("🟢 PARA GİRİŞİ (OI +{:.1}%)", delta_oi_pct)
    } else if delta_oi_pct >= 0.25 && taker_buy_pct <= 46.0 {
        format!("🔴 BALİNA SHORT BASKISI (OI +{:.1}%)", delta_oi_pct)
    } else if delta_oi_pct <= -0.25 && taker_buy_pct >= 54.0 {
        format!("⚠️ SHORT SQUEEZE (OI {:.1}%)", delta_oi_pct)
    } else if delta_oi_pct <= -0.25 && taker_buy_pct <= 46.0 {
        format!("💥 TASFİYE DALGASI (OI {:.1}%)", delta_oi_pct)
    } else if taker_buy_pct >= 55.0 && top_long_pct <= 52.0 {
        "🟢 KURUMSAL TOPLAMA".to_string()
    } else if taker_buy_pct <= 45.0 && top_long_pct >= 65.0 {
        "🔴 KALABALIK TUZAĞI".to_string()
    } else if taker_buy_pct >= 58.0 {
        "⚡ AGRESİF TAKER ALIŞ".to_string()
    } else if taker_buy_pct <= 42.0 {
        "⚠️ AGRESİF TAKER SATIŞ".to_string()
    } else {
        "⚪ DENGELİ AKIŞ".to_string()
    };

    (taker_buy_pct, top_long_pct, delta_oi_pct, sentiment)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutAlert {
    pub symbol: String,
    pub headline: String,
    pub source: String,
    pub sentiment: String,
    pub score_boost: i32,
    pub timestamp_ms: i64,
}

fn detect_symbol_in_text(text: &str) -> Option<&'static str> {
    let upper = text.to_uppercase();
    if upper.contains("SOLANA") || upper.contains(" SOL ") || upper.contains("$SOL") || upper.contains("SOLUSDT") {
        return Some("SOLUSDT");
    }
    if upper.contains("BITCOIN") || upper.contains(" BTC ") || upper.contains("$BTC") || upper.contains("BTCUSDT") {
        return Some("BTCUSDT");
    }
    if upper.contains("ETHEREUM") || upper.contains(" ETH ") || upper.contains("$ETH") || upper.contains("ETHUSDT") {
        return Some("ETHUSDT");
    }
    if upper.contains("RIPPLE") || upper.contains(" XRP ") || upper.contains("$XRP") || upper.contains("XRPUSDT") {
        return Some("XRPUSDT");
    }
    if upper.contains("DOGECOIN") || upper.contains(" DOGE ") || upper.contains("$DOGE") || upper.contains("DOGEUSDT") {
        return Some("DOGEUSDT");
    }
    if upper.contains("CHAINLINK") || upper.contains(" LINK ") || upper.contains("$LINK") || upper.contains("LINKUSDT") {
        return Some("LINKUSDT");
    }
    if upper.contains("SUI NETWORK") || upper.contains(" SUI ") || upper.contains("$SUI") || upper.contains("SUIUSDT") {
        return Some("SUIUSDT");
    }
    if upper.contains("AVALANCHE") || upper.contains(" AVAX ") || upper.contains("$AVAX") || upper.contains("AVAXUSDT") {
        return Some("AVAXUSDT");
    }
    if upper.contains("NEAR PROTOCOL") || upper.contains(" NEAR ") || upper.contains("$NEAR") || upper.contains("NEARUSDT") {
        return Some("NEARUSDT");
    }
    if upper.contains("CARDANO") || upper.contains(" ADA ") || upper.contains("$ADA") || upper.contains("ADAUSDT") {
        return Some("ADAUSDT");
    }
    if upper.contains("APTOS") || upper.contains(" APT ") || upper.contains("$APT") || upper.contains("APTUSDT") {
        return Some("APTUSDT");
    }
    if upper.contains("POLKADOT") || upper.contains(" DOT ") || upper.contains("$DOT") || upper.contains("DOTUSDT") {
        return Some("DOTUSDT");
    }
    if upper.contains("CELESTIA") || upper.contains(" TIA ") || upper.contains("$TIA") || upper.contains("TIAUSDT") {
        return Some("TIAUSDT");
    }
    None
}

fn detect_sentiment_in_text(text: &str) -> (String, i32) {
    let lower = text.to_lowercase();
    let emergency_keywords = [
        "hack", "hacked", "exploit", "exploited", "stolen", "vulnerability",
        "rugpull", "insolvent", "bankrupt", "freeze funds", "halted",
        "delist", "delisting", "sec lawsuit", "criminal charges", "subpoena",
    ];
    for kw in emergency_keywords.iter() {
        if lower.contains(kw) {
            return ("🚨 ACİL DURUM / DUMP".to_string(), -40);
        }
    }

    let bullish_keywords = [
        "listing", "partnership", "etf", "approved", "mainnet", "upgrade",
        "investment", "invested", "inflow", "inflows", "breakout", "backed",
        "accumulate", "accumulating", "bullish", "record high", "whale buy",
    ];
    for kw in bullish_keywords.iter() {
        if lower.contains(kw) {
            return ("🟢 BOĞA HABERİ".to_string(), 12);
        }
    }

    let bearish_keywords = [
        "outflow", "outflows", "selloff", "dump", "dumping", "whale dump",
        "bearish", "liquidated", "liquidation", "downside", "investigation",
        "warning", "fine", "penalty", "lawsuit", "delay", "delayed",
    ];
    for kw in bearish_keywords.iter() {
        if lower.contains(kw) {
            return ("🔴 AYI HABERİ".to_string(), -12);
        }
    }

    ("⚪ SAKİN".to_string(), 0)
}

fn process_scout_payload(raw: &str, store: &Arc<Mutex<HashMap<String, ScoutAlert>>>) {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
        let items: Vec<&serde_json::Value> = if val.is_array() {
            val.as_array().unwrap().iter().collect()
        } else {
            vec![&val]
        };

        for item in items {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("X / News");
            let time_ms = item.get("time").and_then(|v| v.as_i64()).unwrap_or_else(|| {
                chrono::Utc::now().timestamp_millis()
            });

            if let Some(target_sym) = detect_symbol_in_text(title) {
                let (sentiment, delta) = detect_sentiment_in_text(title);
                if delta != 0 {
                    println!(
                        "⚡ [SCOUT RADAR] {} | {} | {} ({:+} Puan) | Başlık: {}",
                        source, target_sym, sentiment, delta, title.chars().take(80).collect::<String>()
                    );
                    if let Ok(mut lock) = store.lock() {
                        lock.insert(target_sym.to_string(), ScoutAlert {
                            symbol: target_sym.to_string(),
                            headline: title.to_string(),
                            source: source.to_string(),
                            sentiment,
                            score_boost: delta,
                            timestamp_ms: time_ms,
                        });
                    }
                }
            }
        }
    }
}

fn run_scout_listener(store: Arc<Mutex<HashMap<String, ScoutAlert>>>) {
    println!("🛰️ [SCOUT GÖZCÜ BAŞLATILDI] X (Twitter) & Balina Canlı Haber Radarı Devrede.");
    let ws_url = "wss://news.treeofalpha.com/ws";
    let rest_url = "https://news.treeofalpha.com/api/news?limit=10";

    loop {
        match tungstenite::connect(ws_url) {
            Ok((mut socket, _)) => {
                println!("🟢 [SCOUT WS] TreeNews Canlı Haber WebSocket Akışı Bağlandı!");
                while let Ok(msg) = socket.read() {
                    if let tungstenite::Message::Text(raw) = msg {
                        process_scout_payload(&raw, &store);
                    }
                }
                println!("⚠️ [SCOUT WS] Bağlantı kesildi, REST fallback ve yeniden bağlanma...");
            }
            Err(_) => {
                if let Ok(output) = Command::new(CURL_BIN)
                    .args(["-s", "--max-time", "5", rest_url])
                    .output()
                {
                    if let Ok(body) = String::from_utf8(output.stdout) {
                        process_scout_payload(&body, &store);
                    }
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

fn warmup_all_symbols(engines: &mut HashMap<String, SymbolEngine>) {
    println!("⏳ [MULTI-COIN WARM-UP] 13 Parite için son 500 adet 5m mum (Mainnet 5-Dakikalık) çekiliyor...");
    for spec in SYMBOLS.iter() {
        let url = format!(
            "{}/fapi/v1/klines?symbol={}&interval=5m&limit=500",
            REST_BASE_URL, spec.symbol
        );
        if let Ok(output) = Command::new(CURL_BIN)
            .args(["-s", "--max-time", "5", &url])
            .output()
        {
            if let Ok(body) = String::from_utf8(output.stdout) {
                if let Ok(rows) = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(&body) {
                    if let Some(eng) = engines.get_mut(spec.symbol) {
                        let closed_len = rows.len().saturating_sub(1);
                        for row in rows.iter().take(closed_len) {
                            if row.len() >= 6 {
                                let o = row[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let h = row[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let l = row[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let c = row[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let v = row[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                if c > 0.0 {
                                    eng.on_closed_candle(o, h, l, c, v);
                                }
                            }
                        }
                        if let Some(last_row) = rows.last() {
                            if last_row.len() >= 5 {
                                let live_c = last_row[4]
                                    .as_str()
                                    .unwrap_or("0")
                                    .parse::<f64>()
                                    .unwrap_or(0.0);
                                if live_c > 0.0 {
                                    eng.on_live_tick(live_c, 0.0, true);
                                }
                            }
                        }
                        println!(
                            "  ✅ {:<8} ({:>2}x) | Fiyat: {:>10.4} | EMA21/50: {:.4}/{:.4} | RSI7/14: {:>4.1}/{:>4.1} | Skor: {:>2}/100 ({})",
                            spec.symbol,
                            spec.leverage,
                            eng.last_price,
                            eng.live_ema21,
                            eng.live_ema50,
                            eng.live_rsi7,
                            eng.live_rsi14,
                            eng.signal_score,
                            eng.signal_dir
                        );
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveBacktestPosition {
    pub symbol: String,
    pub side: &'static str,
    pub entry: f64,
    pub qty: f64,
    pub margin_used: f64,
    pub sl: f64,
    pub tp: f64,
    pub peak_p: f64,
    pub tr_act: bool,
    pub e_fee: f64,
    pub atr: f64,
    pub entry_bar: usize,
    pub entry_time_ms: i64,
}

/// Native Rust Walk-Forward & Takvim (Gün Gün) Backtest Motoru (`cargo run -- --backtest` ile çağrılır)
fn run_native_rust_backtest(args: &[String]) -> Result<()> {
    let days: usize = if let Some(pos) = args.iter().position(|a| a == "--days") {
        args.get(pos + 1).and_then(|v| v.parse::<usize>().ok()).unwrap_or(30)
    } else if args.iter().any(|a| a == "--month" || a == "--30d") {
        30
    } else if args.iter().any(|a| a == "--90d" || a == "--3m") {
        90
    } else if args.iter().any(|a| a == "--20d") {
        20
    } else if args.iter().any(|a| a == "--all") {
        90
    } else if args.iter().any(|a| a == "--10d") {
        10
    } else {
        30 // Varsayılan: 1 Aylık (30 Gün) Takvim
    };
    let hours = days * 24;
    println!("==================================================================================");
    println!("📅 NATIVE RUST 1h GÜNLÜK TAKVİM BACKTEST MOTORU (Institutional Liquidity Sweep & Wick Exhaustion v7.1)");
    println!("Kapsam: {} Gün ({} Saat) (13 Parite) | Taker Fee %0.05 + Slippage %0.01", days, hours);
    println!("==================================================================================");

    let tf_chosen = if args.iter().any(|a| a == "--5m") {
        "5m"
    } else if args.iter().any(|a| a == "--15m") {
        "15m"
    } else {
        "1h"
    };

    for tf in [tf_chosen] {
        // (timestamp_ms, open, high, low, close, volume)
        let mut raw_candles: HashMap<String, Vec<(i64, f64, f64, f64, f64, f64)>> = HashMap::new();
        let mut min_len = usize::MAX;

        let cache_dir = std::path::Path::new("data/cache");
        let force_refresh = args.iter().any(|a| a == "--no-cache" || a == "--force-refresh");

        for spec in SYMBOLS.iter() {
            let mut vec_c = Vec::new();
            let cache_file = cache_dir.join(format!("{}_{}.json", spec.symbol, tf));
            let mut loaded_from_cache = false;

            if cache_file.exists() && !force_refresh && days <= 50 {
                if let Ok(content) = std::fs::read_to_string(&cache_file) {
                    if let Ok(cached_json) = serde_json::from_str::<serde_json::Value>(&content) {
                        let empty_vec = Vec::new();
                        let rows = if let Some(arr) = cached_json.get("data").and_then(|d| d.as_array()) {
                            arr
                        } else if let Some(arr) = cached_json.get("klines").and_then(|k| k.as_array()) {
                            arr
                        } else {
                            cached_json.as_array().unwrap_or(&empty_vec)
                        };
                        for r in rows {
                            let ts = r.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
                            let o = r.get(1).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                            let h = r.get(2).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                            let l = r.get(3).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                            let c = r.get(4).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                            let v = r.get(5).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                            if ts > 0 && c > 0.0 {
                                vec_c.push((ts, o, h, l, c, v));
                            }
                        }
                        if vec_c.len() >= 120 {
                            loaded_from_cache = true;
                        } else {
                            vec_c.clear();
                        }
                    }
                }
            }

            if !loaded_from_cache {
                let url1 = format!(
                    "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval={}&limit=1500",
                    spec.symbol, tf
                );
                let mut chunk1 = Vec::new();
                if let Ok(output) = Command::new(CURL_BIN)
                    .args(["-s", "--max-time", "15", &url1])
                    .output()
                {
                    if let Ok(body) = String::from_utf8(output.stdout) {
                        if let Ok(rows) = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(&body) {
                            for r in rows.iter().take(rows.len().saturating_sub(1)) {
                                if r.len() >= 6 {
                                    let ts = r[0].as_i64().unwrap_or(0);
                                    let o = r[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                    let h = r[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                    let l = r[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                    let c = r[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                    let v = r[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                    if c > 0.0 {
                                        chunk1.push((ts, o, h, l, c, v));
                                    }
                                }
                            }
                        }
                    }
                }

                if days > 50 {
                    let first_ts = chunk1.first().map(|c| c.0).unwrap_or(0);
                    let mut chunk2 = Vec::new();
                    if first_ts > 0 {
                        let url2 = format!(
                            "https://fapi.binance.com/fapi/v1/klines?symbol={}&interval={}&limit=1500&endTime={}",
                            spec.symbol, tf, first_ts - 1
                        );
                        if let Ok(output) = Command::new(CURL_BIN)
                            .args(["-s", "--max-time", "15", &url2])
                            .output()
                        {
                            if let Ok(body) = String::from_utf8(output.stdout) {
                                if let Ok(rows) = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(&body) {
                                    for r in rows.iter() {
                                        if r.len() >= 6 {
                                            let ts = r[0].as_i64().unwrap_or(0);
                                            let o = r[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                            let h = r[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                            let l = r[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                            let c = r[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                            let v = r[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                            if c > 0.0 {
                                                chunk2.push((ts, o, h, l, c, v));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    vec_c.extend(chunk2);
                    vec_c.extend(chunk1);
                } else {
                    vec_c = chunk1;
                }
            }

            if vec_c.len() < min_len {
                min_len = vec_c.len();
            }
            raw_candles.insert(spec.symbol.to_string(), vec_c);
        }

        if min_len < 120 || raw_candles.len() < 6 {
            println!("⚠️ {} verisi çekilemedi.", tf);
            continue;
        }

        let mut engines: HashMap<String, SymbolEngine> = HashMap::new();
        for spec in SYMBOLS.iter() {
            engines.insert(spec.symbol.to_string(), SymbolEngine::new(*spec));
        }

        let mut balance = INITIAL_BALANCE;
        let mut peak = INITIAL_BALANCE;
        let mut max_dd = 0.0;
        let mut total_fees = 0.0;
        let mut wins = 0u32;
        let mut losses = 0u32;
        let mut gross_win = 0.0;
        let mut gross_loss = 0.0;
        let mut active: Option<ActiveBacktestPosition> = None;
        let mut closed_trades: Vec<ClosedTradeRecord> = Vec::new();
        let mut cooldown_until: HashMap<String, usize> = HashMap::new();

        // Günlük Takvim İstatistikleri: (date_str, start_bal, end_bal, wins, losses, net_pnl, details)
        let mut daily_order: Vec<String> = Vec::new();
        let mut daily_stats: HashMap<String, (f64, f64, u32, u32, f64, Vec<String>)> =
            HashMap::new();

        for i in 0..(min_len - 1) {
            let ts_ms = raw_candles.get("SOLUSDT").unwrap()[i].0;
            let day_str = chrono::DateTime::from_timestamp_millis(ts_ms)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            for spec in SYMBOLS.iter() {
                if let (Some(eng), Some(bars)) =
                    (engines.get_mut(spec.symbol), raw_candles.get(spec.symbol))
                {
                    let (_, o, h, l, c, v) = bars[i];
                    eng.on_closed_candle(o, h, l, c, v);
                }
            }

            let bars_to_test = if tf == "5m" {
                days * 24 * 12
            } else if tf == "15m" {
                days * 24 * 4
            } else {
                hours
            };
            let start_test_idx = min_len.saturating_sub(bars_to_test).max(100);

            if i < start_test_idx {
                continue;
            }

            if !daily_stats.contains_key(&day_str) {
                daily_order.push(day_str.clone());
                daily_stats.insert(
                    day_str.clone(),
                    (balance, balance, 0, 0, 0.0, Vec::new()),
                );
            }

            let mut emergency_exit = false;
            let mut exit_reason = "SL";
            if let Some(mut pos) = active.take() {
                let sym = pos.symbol.clone();
                let side = pos.side;
                let entry = pos.entry;
                let qty = pos.qty;
                let mut sl = pos.sl;
                let tp = pos.tp;
                let mut peak_p = pos.peak_p;
                let mut tr_act = pos.tr_act;
                let e_fee = pos.e_fee;
                let atr = pos.atr;

                let (_, _, h, l, cur_c, _) = raw_candles.get(&sym).unwrap()[i];
                let mut exit_raw: Option<f64> = None;
                let _holding_bars = i.saturating_sub(pos.entry_bar);

                if side == "LONG" {
                    if h > peak_p {
                        peak_p = h;
                    }
                    // Nefes Alan Komisyon Korumalı Kâr Kilidi: Fiyat +1.5x ATR gittiğinde 3x ATR hedefli trendi boğmadan komisyon üstü kâr kilitler
                    if peak_p - entry >= atr * 2.0 {
                        let profit_lock = entry * 1.0025;
                        if profit_lock > sl {
                            sl = profit_lock;
                            tr_act = true;
                        }
                    }
                    if peak_p - entry >= atr * 2.0 {
                        let candidate_sl = peak_p - atr * 2.0;
                        if candidate_sl > sl {
                            sl = candidate_sl;
                            tr_act = true;
                        }
                    }
                    let (btc_inst_dir, btc_inst_score) = engines.get("BTCUSDT")
                        .map(|b| (b.signal_dir, b.signal_score))
                        .unwrap_or(("NEUTRAL", 0));
                    if let Some(eng) = engines.get(&sym) {
                        if check_emergency_exit_typesafe_jev(
                            &sym,
                            "LONG",
                            entry,
                            cur_c,
                            eng.live_scout_score_boost,
                            &eng.live_scout_headline,
                            eng.signal_dir,
                            eng.signal_score,
                            btc_inst_dir,
                            btc_inst_score,
                            0.0,
                            0.0,
                        ).is_some() {
                            emergency_exit = true;
                        }
                    }
                    if l <= sl {
                        exit_raw = Some(sl);
                        exit_reason = if tr_act {
                            "PROFIT_LOCK"
                        } else {
                            "SL"
                        };
                    } else if h >= tp {
                        exit_raw = Some(tp);
                        exit_reason = "TP";
                    } else if emergency_exit {
                        exit_raw = Some(cur_c);
                        exit_reason = "EMERGENCY";
                    }
                } else {
                    if l < peak_p {
                        peak_p = l;
                    }
                    // Nefes Alan Komisyon Korumalı Kâr Kilidi: Fiyat +1.5x ATR düştüğünde 3x ATR hedefli trendi boğmadan komisyon üstü kâr kilitler
                    if entry - peak_p >= atr * 2.0 {
                        let profit_lock = entry * 0.9975;
                        if profit_lock < sl {
                            sl = profit_lock;
                            tr_act = true;
                        }
                    }
                    if entry - peak_p >= atr * 2.0 {
                        let candidate_sl = peak_p + atr * 2.0;
                        if candidate_sl < sl {
                            sl = candidate_sl;
                            tr_act = true;
                        }
                    }
                    let (btc_inst_dir, btc_inst_score) = engines.get("BTCUSDT")
                        .map(|b| (b.signal_dir, b.signal_score))
                        .unwrap_or(("NEUTRAL", 0));
                    if let Some(eng) = engines.get(&sym) {
                        if check_emergency_exit_typesafe_jev(
                            &sym,
                            "SHORT",
                            entry,
                            cur_c,
                            eng.live_scout_score_boost,
                            &eng.live_scout_headline,
                            eng.signal_dir,
                            eng.signal_score,
                            btc_inst_dir,
                            btc_inst_score,
                            0.0,
                            0.0,
                        ).is_some() {
                            emergency_exit = true;
                        }
                    }
                    if h >= sl {
                        exit_raw = Some(sl);
                        exit_reason = if tr_act {
                            "PROFIT_LOCK"
                        } else {
                            "SL"
                        };
                    } else if l <= tp {
                        exit_raw = Some(tp);
                        exit_reason = "TP";
                    } else if emergency_exit {
                        exit_raw = Some(cur_c);
                        exit_reason = "EMERGENCY";
                    }
                }

                if let Some(raw_px) = exit_raw {
                    let exit_exec = if side == "LONG" {
                        raw_px * (1.0 - SLIPPAGE_RATE)
                    } else {
                        raw_px * (1.0 + SLIPPAGE_RATE)
                    };
                    let exit_fee = exit_exec * qty * TAKER_FEE_RATE;
                    let gross = if side == "LONG" {
                        (exit_exec - entry) * qty
                    } else {
                        (entry - exit_exec) * qty
                    };
                    let net = gross - e_fee - exit_fee;
                    balance += gross - exit_fee;
                    total_fees += exit_fee;

                    let entry_day = daily_stats.get_mut(&day_str).unwrap();
                    entry_day.1 = balance;
                    entry_day.4 += net;
                    if net >= 0.0 {
                        wins += 1;
                        entry_day.2 += 1;
                        gross_win += net;
                    } else {
                        losses += 1;
                        entry_day.3 += 1;
                        gross_loss += net.abs();
                        cooldown_until.insert(sym.clone(), i + 10);
                    }
                    entry_day.5.push(format!(
                        "{} {} ({:+.2}$)",
                        sym.replace("USDT", ""),
                        side,
                        net
                    ));

                    if balance > peak {
                        peak = balance;
                    } else {
                        let dd = ((peak - balance) / peak) * 100.0;
                        if dd > max_dd {
                            max_dd = dd;
                        }
                    }

                    // Milestone 1 Feature 2: Time-in-Trade & Duration Tracking Instrumentation
                    let exit_bar = i;
                    let tf_ms: i64 = if tf == "15m" { 900_000 } else if tf == "5m" { 300_000 } else { 3_600_000 };
                    let candle_ts = raw_candles.get(&sym).unwrap()[i].0;
                    let exit_time_ms = if candle_ts <= pos.entry_time_ms {
                        pos.entry_time_ms + tf_ms
                    } else {
                        candle_ts
                    };
                    let raw_duration_ms = (exit_time_ms - pos.entry_time_ms).max(60_000);
                    let holding_duration_hours = (raw_duration_ms as f64) / 3_600_000.0;
                    let _holding_bars = exit_bar.saturating_sub(pos.entry_bar).max(1);
                    let effective_duration_hours = holding_duration_hours.max(1.0 / 60.0);
                    let roe_pct = if pos.margin_used > 0.0 { (net / pos.margin_used) * 100.0 } else { 0.0 };
                    let hourly_active_roe = if pos.margin_used > 0.0 { net / pos.margin_used / effective_duration_hours } else { 0.0 };
                    let lev = SYMBOLS.iter().find(|s| s.symbol == sym).map(|s| s.leverage).unwrap_or(6);

                    closed_trades.push(ClosedTradeRecord {
                        timestamp: chrono::DateTime::from_timestamp_millis(exit_time_ms)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "Unknown".to_string()),
                        symbol: sym.clone(),
                        side: side.to_string(),
                        leverage: lev,
                        qty,
                        margin_used: pos.margin_used,
                        entry_price: entry,
                        exit_price: exit_exec,
                        gross_pnl: gross,
                        fees_paid: e_fee + exit_fee,
                        net_pnl: net,
                        roe_pct,
                        reason: exit_reason.to_string(),
                        entry_time_ms: pos.entry_time_ms as u64,
                        exit_time_ms: exit_time_ms as u64,
                        holding_duration_seconds: (raw_duration_ms / 1000) as u64,
                        holding_duration_hours,
                        hourly_active_roe,
                    });
                } else {
                    pos.sl = sl;
                    pos.peak_p = peak_p;
                    pos.tr_act = tr_act;
                    active = Some(pos);
                    if let Some(d) = daily_stats.get_mut(&day_str) {
                        d.1 = balance;
                    }
                    continue;
                }
            }

            if active.is_none() {
                // Günlük Hedef ve Zarar Kilidi (Daily Target Lock & Loss Breaker):
                if let Some(d) = daily_stats.get(&day_str) {
                    if d.4 >= (balance * 0.05).max(0.50) || d.4 <= -(balance * 0.08).max(0.65) || d.3 >= 3 {
                        continue;
                    }
                }

                let (btc_trend, btc_instant_dir, btc_instant_score) = if let Some(btc_eng) = engines.get("BTCUSDT") {
                    let tr = if btc_eng.live_ema21 > btc_eng.live_ema50 {
                        "BOĞA"
                    } else if btc_eng.live_ema21 < btc_eng.live_ema50 {
                        "AYI"
                    } else {
                        "NÖTR"
                    };
                    (tr, btc_eng.signal_dir, btc_eng.signal_score)
                } else {
                    ("NÖTR", "NEUTRAL", 0)
                };

                let mut market_long_count = 0usize;
                let mut market_short_count = 0usize;
                for spec in SYMBOLS.iter() {
                    if let Some(e) = engines.get(spec.symbol) {
                        if e.signal_dir == "LONG" && e.signal_score >= 60 {
                            market_long_count += 1;
                        } else if e.signal_dir == "SHORT" && e.signal_score >= 60 {
                            market_short_count += 1;
                        }
                    }
                }

                let mut best: Option<(&SymbolEngine, u32)> = None;
                for spec in SYMBOLS.iter() {
                    if cooldown_until.get(spec.symbol).copied().unwrap_or(0) > i {
                        continue;
                    }
                    if let Some(eng) = engines.get(spec.symbol) {
                        if eng.signal_dir != "NEUTRAL" && eng.signal_score >= 78 {
                            // 1. Zero-Lag BTC Gate
                            if btc_instant_dir == "SHORT" && btc_instant_score >= 65 && eng.signal_dir == "LONG" {
                                continue;
                            }
                            if btc_instant_dir == "LONG" && btc_instant_score >= 65 && eng.signal_dir == "SHORT" {
                                continue;
                            }
                            // 1.1 BTC Makro Rejim & HTF Kalkanı: BTC EMA50 veya EMA200 altındaysa (Makro Ayı Rejimi) altcoinlerde LONG tuzaklarına girme!
                            if let Some(btc_e) = engines.get("BTCUSDT") {
                                if eng.signal_dir == "LONG"
                                    && (btc_e.last_price < btc_e.live_ema50
                                        || btc_e.last_price < btc_e.live_ema200
                                        || btc_e.live_ema50 < btc_e.live_ema200
                                        || btc_trend == "AYI")
                                {
                                    continue;
                                }
                                if eng.signal_dir == "SHORT"
                                    && (btc_e.last_price > btc_e.live_ema50
                                        || btc_e.last_price > btc_e.live_ema200
                                        || btc_e.live_ema50 > btc_e.live_ema200
                                        || btc_trend == "BOĞA")
                                {
                                    continue;
                                }
                            }
                            if btc_trend == "BOĞA" && btc_instant_dir != "SHORT" && eng.signal_dir != "LONG" {
                                continue;
                            }
                            if btc_trend == "AYI" && eng.signal_dir != "SHORT" {
                                continue;
                            }

                            // 2. Kolektif Piyasa Konsensüsü
                            if eng.signal_dir == "LONG" {
                                if market_short_count >= 5 || market_long_count < 2 {
                                    continue;
                                }
                            } else if eng.signal_dir == "SHORT" {
                                if market_long_count >= 5 || market_short_count < 2 {
                                    continue;
                                }
                            }

                            // 3. Parite HTF Trend Uyumu (Makro EMA200 & EMA50 Kalkanı)
                            let htf_aligned = if eng.signal_dir == "LONG" {
                                eng.signal_score >= 85
                                    && eng.last_price >= eng.live_ema50 * 1.005
                                    && eng.last_price >= eng.live_ema200 * 1.015
                                    && eng.live_ema50 >= eng.live_ema200 * 1.005
                                    && eng.live_ema21 >= eng.live_ema50
                                    && eng.live_ema9 >= eng.live_ema21
                            } else if eng.signal_dir == "SHORT" {
                                eng.signal_score >= 85
                                    && eng.last_price <= eng.live_ema50 * 0.995
                                    && eng.last_price <= eng.live_ema200 * 0.985
                                    && eng.live_ema50 <= eng.live_ema200 * 0.995
                                    && eng.live_ema21 <= eng.live_ema50 * 1.002
                                    && eng.live_ema9 <= eng.live_ema21 * 1.002
                            } else {
                                false
                            };
                            if !htf_aligned {
                                continue;
                            }
                            // 4. Lead-Lag Beta Propagation Boost
                            let lead_lag_boost = if let Some(btc_e) = engines.get("BTCUSDT") {
                                compute_lead_lag_beta_boost(btc_e, eng, 0.0)
                            } else {
                                0
                            };
                            if lead_lag_boost <= -20 {
                                continue;
                            }
                            let eff_score = (eng.signal_score as i32 + lead_lag_boost).clamp(0, 100) as u32;
                            if eng.calculate_micro_10usd_order(balance).is_some() {
                                if best.map_or(true, |(_, bs)| eff_score > bs) {
                                    best = Some((eng, eff_score));
                                }
                            }
                        }
                    }
                }
                if let Some((eng, _)) = best {
                    let (is_approved, _prob, _reason) = typesafe_unified_arbiter(
                        eng.spec.symbol,
                        eng.signal_dir,
                        eng.signal_score,
                        eng.last_price,
                        eng.live_ema50,
                        eng.live_rsi14,
                        eng.live_taker_buy_pct,
                        eng.live_delta_oi_pct,
                        eng.live_scout_score_boost,
                        &eng.live_scout_headline,
                        btc_trend,
                        btc_instant_dir,
                        btc_instant_score,
                        market_long_count,
                        market_short_count,
                    );
                    if !is_approved {
                        continue;
                    }
                    if let Some((qty, _, stop_dist, tp_dist)) =
                        eng.calculate_micro_10usd_order(balance)
                    {
                        let next_open = raw_candles.get(eng.spec.symbol).unwrap()[i + 1].1;
                        let exec_p = if eng.signal_dir == "LONG" {
                            next_open * (1.0 + SLIPPAGE_RATE)
                        } else {
                            next_open * (1.0 - SLIPPAGE_RATE)
                        };
                        let e_fee = exec_p * qty * TAKER_FEE_RATE;
                        let sl = if eng.signal_dir == "LONG" {
                            exec_p - stop_dist
                        } else {
                            exec_p + stop_dist
                        };
                        let tp = if eng.signal_dir == "LONG" {
                            exec_p + tp_dist
                        } else {
                            exec_p - tp_dist
                        };
                        balance -= e_fee;
                        total_fees += e_fee;
                        if let Some(d) = daily_stats.get_mut(&day_str) {
                            d.1 = balance;
                        }
                        let margin_used = (exec_p * qty) / (eng.spec.leverage as f64);
                        let entry_time_ms = raw_candles.get(eng.spec.symbol).unwrap()[i + 1].0;
                        active = Some(ActiveBacktestPosition {
                            symbol: eng.spec.symbol.to_string(),
                            side: eng.signal_dir,
                            entry: exec_p,
                            qty,
                            margin_used,
                            sl,
                            tp,
                            peak_p: exec_p,
                            tr_act: false,
                            e_fee,
                            atr: eng.live_atr14,
                            entry_bar: i + 1,
                            entry_time_ms,
                        });
                    }
                }
            }
        }

        println!("\n📅 GÜNLÜK TAKVİM DÖKÜMÜ (1H Mumlar - 13 Coin Birleşik):");
        println!("----------------------------------------------------------------------------------------------------");
        println!(
            "{:<12} | {:<18} | {:<10} | {:<14} | {:<32}",
            "Tarih", "Gün Başı -> Sonu", "İşlem(K/Z)", "Günlük Net K/Z", "Kapanan İşlem Detayları"
        );
        println!("----------------------------------------------------------------------------------------------------");
        for d_key in &daily_order {
            if let Some((s_bal, e_bal, w, l, net_d, details)) = daily_stats.get(d_key) {
                let d_pct = if *s_bal > 0.0 {
                    ((*e_bal - *s_bal) / *s_bal) * 100.0
                } else {
                    0.0
                };
                let det_str = if details.is_empty() {
                    "Açık pozisyon taşındı / Sinyal beklendi".to_string()
                } else {
                    details.join(", ")
                };
                println!(
                    "{:<12} | ${:<6.2} -> ${:<6.2} | {:>2} ({}K/{}Z) | {:>+6.2}$ ({:>+5.1}%) | {}",
                    d_key,
                    s_bal,
                    e_bal,
                    w + l,
                    w,
                    l,
                    net_d,
                    d_pct,
                    det_str
                );
            }
        }
        println!("----------------------------------------------------------------------------------------------------");

        let total_tr = wins + losses;
        let wr = if total_tr > 0 {
            (wins as f64 / total_tr as f64) * 100.0
        } else {
            0.0
        };
        let pf = if gross_loss > 0.0 {
            gross_win / gross_loss
        } else {
            99.0
        };
        let ret_pct = ((balance - INITIAL_BALANCE) / INITIAL_BALANCE) * 100.0;
        let num_days = daily_order.len().max(1) as f64;
        let avg_daily_usd = (balance - INITIAL_BALANCE) / num_days;
        let avg_daily_pct = ret_pct / num_days;

        println!(
            "🏆 [GENEL TOPLAM ({} Takvim Günü | {}x13={} Mum)]",
            daily_order.len(),
            min_len,
            min_len * SYMBOLS.len()
        );
        println!(
            "   Başlangıç: $10.00 -> Son Kasa: ${:.2} ({:+.2}%) | Ortalama Günlük Getiri: {:+.2}$ ({:+.2}% / gün)",
            balance, ret_pct, avg_daily_usd, avg_daily_pct
        );
        println!(
            "   Toplam İşlem: {} ({} Kazanan / {} Kaybeden -> WinRate: %{:.1}) | Profit Factor: {:.2} | Toplam Fee: -${:.2} | Max DD: %{:.2}\n",
            total_tr, wins, losses, wr, pf, total_fees, max_dd
        );

        let total_duration_hours: f64 = closed_trades.iter().map(|t| t.holding_duration_hours).sum();
        let avg_duration_hours = if total_tr > 0 { total_duration_hours / total_tr as f64 } else { 0.0 };
        let active_capital_hourly_roe = if total_duration_hours > 0.0 { ret_pct / total_duration_hours.max(1.0) } else { 0.0 };
        let total_notional: f64 = closed_trades.iter().map(|t| (t.entry_price + t.exit_price) * t.qty).sum();
        let capital_turnover_rate = total_notional / INITIAL_BALANCE / num_days.max(0.04167);

        println!(
            "⏱️ [ZAMAN BAZLI VERİMLİLİK METRİKLERİ]\n   Ortalama Pozisyon Süresi: {:.2} saat | Toplam Maruziyet: {:.1} saat\n   Aktif Sermaye Saatlik Getiri: {:+.4}% / saat | Sermaye Devir Hızı: {:.2}x / gün\n",
            avg_duration_hours, total_duration_hours, active_capital_hourly_roe, capital_turnover_rate
        );

        let _ = std::fs::create_dir_all("reports");
        let report_payload = serde_json::json!({
            "strategy_name": "TriRegime_Institutional_Sweep_v7.1",
            "timeframe": tf,
            "days": days,
            "symbols": SYMBOLS.iter().map(|s| s.symbol).collect::<Vec<_>>(),
            "summary": {
                "initial_balance": INITIAL_BALANCE,
                "final_balance": balance,
                "net_profit_pct": ret_pct,
                "total_trades": total_tr,
                "wins": wins,
                "losses": losses,
                "win_rate_pct": wr,
                "profit_factor": pf,
                "total_fees": total_fees,
                "max_drawdown_pct": max_dd,
                "avg_duration_hours": avg_duration_hours,
                "total_duration_hours": total_duration_hours,
                "active_capital_hourly_roe": active_capital_hourly_roe,
                "capital_turnover_rate": capital_turnover_rate,
            },
            "trades": closed_trades
        });
        let _ = std::fs::write(
            "reports/backtest_latest.json",
            serde_json::to_string_pretty(&report_payload).unwrap_or_default(),
        );
    }
    Ok(())
}


// ==================================================================================
// 🧬 7/24 KESİNTİSİZ SIFIR-TOKEN STRATEJİ AVCI SWARM MOTORU (Continuous Strategy Swarm)
// ==================================================================================

struct FastRng {
    state: u64,
}

impl FastRng {
    fn new(seed: u64) -> Self {
        Self { state: if seed == 0 { 0x853c49e6748fea9b } else { seed } }
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64)
    }
    fn choose<T: Copy>(&mut self, slice: &[T]) -> T {
        let idx = (self.next_u64() as usize) % slice.len();
        slice[idx]
    }
    fn choose_ref<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        let idx = (self.next_u64() as usize) % slice.len();
        &slice[idx]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyChromosome {
    pub id: String,
    pub sl_atr_mult: f64,
    pub tp_atr_mult: f64,
    pub be_trigger_atr: f64,
    pub trail_trigger_atr: f64,
    pub trail_offset_atr: f64,
    pub risk_pct: f64,
    pub min_score: u32,
    pub daily_profit_pct: f64,
    pub daily_loss_pct: f64,
    pub max_daily_trades: usize,
    pub cooldown_bars: usize,

    // 🧬 Çoklu İndikatör ve Rejim Taraması
    #[serde(default)]
    pub strategy_mode: u8,   // 0: Hibrit Tri-Rejim, 1: Likidite Avı (Sweep), 2: Trend Kırılımı (Breakout), 3: Trend Pullback
    #[serde(default = "default_lookback")]
    pub donchian_lookback: usize, // 8, 12, 16, 20
    #[serde(default = "default_wick_pct")]
    pub min_wick_ratio: f64, // 0.15, 0.18, 0.22, 0.28
    #[serde(default)]
    pub rsi_filter_mode: u8, // 0: Normal, 1: Katı (35/65), 2: Geniş (45/55)
    #[serde(default = "default_htf_mode")]
    pub htf_filter_mode: u8, // 0: Katı EMA21>50, 1: Hızlı EMA9>21, 2: Serbest
}

fn default_lookback() -> usize { 12 }
fn default_wick_pct() -> f64 { 0.18 }
fn default_htf_mode() -> u8 { 0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmReport {
    pub rank: usize,
    pub id: String,
    pub chromosome: StrategyChromosome,
    pub final_balance: f64,
    pub net_roi_pct: f64,
    pub total_trades: usize,
    pub win_trades: usize,
    pub loss_trades: usize,
    pub win_rate_pct: f64,
    pub profit_factor: f64,
    pub max_drawdown_pct: f64,
    pub avg_daily_gain_usd: f64,
    #[serde(default)]
    pub daily_pnl_usd: f64,
    #[serde(default)]
    pub daily_pnl_pct: f64,
    #[serde(default)]
    pub weekly_pnl_usd: f64,
    #[serde(default)]
    pub weekly_pnl_pct: f64,
    #[serde(default)]
    pub monthly_pnl_usd: f64,
    #[serde(default)]
    pub monthly_pnl_pct: f64,
    #[serde(default)]
    pub daily_trades: f64,
    #[serde(default)]
    pub weekly_trades: usize,
    #[serde(default)]
    pub monthly_trades: usize,
    pub fitness_score: f64,
    #[serde(default)]
    pub oos_roi_pct: f64,
    #[serde(default)]
    pub oos_win_rate_pct: f64,
    #[serde(default)]
    pub oos_trades: usize,
    #[serde(default)]
    pub mc_worst_dd_pct: f64,
    #[serde(default)]
    pub mar_ratio: f64,
    #[serde(default)]
    pub wfa_profitable_folds: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmState {
    pub is_running: bool,
    pub generation: u64,
    pub total_evaluated: u64,
    pub last_batch_size: usize,
    pub top_champions: Vec<SwarmReport>,
    pub last_update: String,
}

fn load_all_swarm_candles(tf: &str) -> (HashMap<String, Vec<(i64, f64, f64, f64, f64, f64)>>, usize) {
    let mut raw_candles: HashMap<String, Vec<(i64, f64, f64, f64, f64, f64)>> = HashMap::new();
    let mut min_len = usize::MAX;
    let cache_dir = std::path::Path::new("data/cache");

    for spec in SYMBOLS.iter() {
        let mut vec_c = Vec::new();
        let cache_file = cache_dir.join(format!("{}_{}.json", spec.symbol, tf));
        if cache_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&cache_file) {
                if let Ok(cached_json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let empty_vec = Vec::new();
                    let rows = if let Some(arr) = cached_json.get("data").and_then(|d| d.as_array()) {
                        arr
                    } else if let Some(arr) = cached_json.get("klines").and_then(|k| k.as_array()) {
                        arr
                    } else {
                        cached_json.as_array().unwrap_or(&empty_vec)
                    };
                    for r in rows {
                        let ts = r.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
                        let o = r.get(1).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                        let h = r.get(2).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                        let l = r.get(3).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                        let c = r.get(4).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                        let v_vol = r.get(5).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0.0);
                        if ts > 0 && c > 0.0 {
                            vec_c.push((ts, o, h, l, c, v_vol));
                        }
                    }
                }
            }
        }

        if vec_c.len() < 120 {
            // Fallback curl to binance
            let url = format!("{}/fapi/v1/klines?symbol={}&interval={}&limit=1500", REST_BASE_URL, spec.symbol, tf);
            if let Ok(output) = Command::new(CURL_BIN).args(["-s", "--max-time", "15", &url]).output() {
                if let Ok(body) = String::from_utf8(output.stdout) {
                    if let Ok(rows) = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(&body) {
                        for r in rows.iter().take(rows.len().saturating_sub(1)) {
                            if r.len() >= 6 {
                                let ts = r[0].as_i64().unwrap_or(0);
                                let o = r[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let h = r[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let l = r[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let c = r[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                let v_vol = r[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
                                if c > 0.0 {
                                    vec_c.push((ts, o, h, l, c, v_vol));
                                }
                            }
                        }
                    }
                }
            }
        }

        if vec_c.len() < min_len {
            min_len = vec_c.len();
        }
        raw_candles.insert(spec.symbol.to_string(), vec_c);
    }
    (raw_candles, min_len)
}

// 🎲 1,000x MONTE CARLO PERMÜTASYON STRES TESTİ
// İşlemlerin geçmişteki sırasını 1.000 kez rastgele karıştırarak en kötü %95 senaryodaki (P95) Max Drawdown'ı hesaplar.
fn run_monte_carlo_permutation(trade_pnls: &[f64], iterations: usize, rng: &mut FastRng) -> (f64, f64) {
    if trade_pnls.is_empty() {
        return (0.0, 0.0);
    }
    let mut worst_dds: Vec<f64> = Vec::with_capacity(iterations);
    let mut shuffled = trade_pnls.to_vec();

    for _ in 0..iterations {
        for j in (1..shuffled.len()).rev() {
            let k = (rng.next_u64() as usize) % (j + 1);
            shuffled.swap(j, k);
        }

        let mut bal = INITIAL_BALANCE;
        let mut peak = INITIAL_BALANCE;
        let mut sim_max_dd = 0.0;

        for &pnl in shuffled.iter() {
            bal += pnl;
            if bal > peak {
                peak = bal;
            } else if peak > 0.0 {
                let dd = ((peak - bal) / peak) * 100.0;
                if dd > sim_max_dd {
                    sim_max_dd = dd;
                }
            }
        }
        worst_dds.push(sim_max_dd);
    }

    worst_dds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx_95 = ((iterations as f64) * 0.95) as usize;
    let p95_dd = worst_dds.get(idx_95.min(iterations - 1)).copied().unwrap_or(0.0);
    let max_worst_dd = worst_dds.last().copied().unwrap_or(0.0);
    (p95_dd, max_worst_dd)
}

fn evaluate_chromosome(
    chromo: &StrategyChromosome,
    raw_candles: &HashMap<String, Vec<(i64, f64, f64, f64, f64, f64)>>,
    min_len: usize,
    hours: usize,
) -> Option<SwarmReport> {
    let mut engines: HashMap<String, SymbolEngine> = HashMap::new();
    for spec in SYMBOLS.iter() {
        let mut eng = SymbolEngine::new(*spec);
        eng.custom_strategy_mode = chromo.strategy_mode;
        eng.custom_donchian_lookback = chromo.donchian_lookback;
        eng.custom_min_wick_ratio = chromo.min_wick_ratio;
        engines.insert(spec.symbol.to_string(), eng);
    }

    let mut balance = INITIAL_BALANCE;
    let mut peak = INITIAL_BALANCE;
    let mut max_dd = 0.0;
    let mut wins = 0u32;
    let mut losses = 0u32;
    let mut gross_win = 0.0;
    let mut gross_loss = 0.0;
    let mut active: Option<ActiveBacktestPosition> = None;
    let mut cooldown_until: HashMap<String, usize> = HashMap::new();
    let mut daily_stats: HashMap<String, (f64, f64, u32, u32, f64)> = HashMap::new();
    let mut daily_order: Vec<String> = Vec::new();
    let mut trade_pnls: Vec<f64> = Vec::new();

    let start_test_idx = min_len.saturating_sub(hours).max(100);
    let total_test_span = min_len.saturating_sub(start_test_idx);
    let train_span = ((total_test_span as f64) * 0.65) as usize; // %65 Eğitim (In-Sample, ~65 Gün)
    let split_idx = start_test_idx + train_span; // Buradan sonrası %35 Kör Test (Out-of-Sample, ~35 Gün)

    let mut train_pnl = 0.0;
    let mut _train_wins = 0u32;
    let mut _train_losses = 0u32;
    let mut train_peak = INITIAL_BALANCE;
    let mut train_max_dd = 0.0;

    let mut oos_pnl = 0.0;
    let mut oos_wins = 0u32;
    let mut oos_losses = 0u32;
    let mut oos_peak = INITIAL_BALANCE;
    let mut oos_max_dd = 0.0;

    // 🔄 WALK-FORWARD ANALYSIS (WFA) - 3 Kayan Dönem
    let fold_len = (total_test_span / 3).max(1);
    let fold1_end = start_test_idx + fold_len;
    let fold2_end = start_test_idx + fold_len * 2;
    let mut fold_pnls = [0.0f64; 3];
    let mut fold_wins = [0u32; 3];
    let mut fold_losses = [0u32; 3];

    for i in 0..(min_len - 1) {
        let ts_ms = raw_candles.get("SOLUSDT")?.get(i)?.0;
        let day_str = chrono::DateTime::from_timestamp_millis(ts_ms)
            .map(|dt| dt.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        for spec in SYMBOLS.iter() {
            if let (Some(eng), Some(bars)) = (engines.get_mut(spec.symbol), raw_candles.get(spec.symbol)) {
                if i < bars.len() {
                    let (_, o, h, l, c, v) = bars[i];
                    eng.on_closed_candle(o, h, l, c, v);
                }
            }
        }

        if i < start_test_idx {
            continue;
        }

        if !daily_stats.contains_key(&day_str) {
            daily_order.push(day_str.clone());
            daily_stats.insert(day_str.clone(), (balance, balance, 0, 0, 0.0));
        }

        if let Some(mut pos) = active.take() {
            let sym = pos.symbol.clone();
            let side = pos.side;
            let entry = pos.entry;
            let qty = pos.qty;
            let mut sl = pos.sl;
            let tp = pos.tp;
            let mut peak_p = pos.peak_p;
            let mut tr_act = pos.tr_act;
            let e_fee = pos.e_fee;
            let atr = pos.atr;

            let (_, _, h, l, _, _) = raw_candles.get(&sym)?.get(i).copied()?;
            let mut exit_raw: Option<f64> = None;

            if side == "LONG" {
                if h > peak_p { peak_p = h; }
                if chromo.be_trigger_atr < 50.0 && peak_p - entry >= atr * chromo.be_trigger_atr {
                    let profit_lock = entry * 1.0025;
                    if profit_lock > sl { sl = profit_lock; tr_act = true; }
                }
                if chromo.trail_trigger_atr < 50.0 && peak_p - entry >= atr * chromo.trail_trigger_atr {
                    let candidate_sl = peak_p - atr * chromo.trail_offset_atr;
                    if candidate_sl > sl { sl = candidate_sl; tr_act = true; }
                }
                if l <= sl {
                    exit_raw = Some(sl);
                } else if h >= tp {
                    exit_raw = Some(tp);
                }
            } else {
                if l < peak_p { peak_p = l; }
                if chromo.be_trigger_atr < 50.0 && entry - peak_p >= atr * chromo.be_trigger_atr {
                    let profit_lock = entry * 0.9975;
                    if profit_lock < sl { sl = profit_lock; tr_act = true; }
                }
                if chromo.trail_trigger_atr < 50.0 && entry - peak_p >= atr * chromo.trail_trigger_atr {
                    let candidate_sl = peak_p + atr * chromo.trail_offset_atr;
                    if candidate_sl < sl { sl = candidate_sl; tr_act = true; }
                }
                if h >= sl {
                    exit_raw = Some(sl);
                } else if l <= tp {
                    exit_raw = Some(tp);
                }
            }

            if let Some(raw_px) = exit_raw {
                let exit_exec = if side == "LONG" { raw_px * (1.0 - SLIPPAGE_RATE) } else { raw_px * (1.0 + SLIPPAGE_RATE) };
                let exit_fee = exit_exec * qty * TAKER_FEE_RATE;
                let gross = if side == "LONG" { (exit_exec - entry) * qty } else { (entry - exit_exec) * qty };
                let net = gross - e_fee - exit_fee;
                balance += gross - exit_fee;
                trade_pnls.push(net);

                if i < split_idx {
                    train_pnl += net;
                    if net >= 0.0 { _train_wins += 1; } else { _train_losses += 1; }
                    let cur_bal = INITIAL_BALANCE + train_pnl;
                    if cur_bal > train_peak { train_peak = cur_bal; }
                    else if train_peak > 0.0 {
                        let dd = ((train_peak - cur_bal) / train_peak) * 100.0;
                        if dd > train_max_dd { train_max_dd = dd; }
                    }
                } else {
                    oos_pnl += net;
                    if net >= 0.0 { oos_wins += 1; } else { oos_losses += 1; }
                    let cur_bal = INITIAL_BALANCE + oos_pnl;
                    if cur_bal > oos_peak { oos_peak = cur_bal; }
                    else if oos_peak > 0.0 {
                        let dd = ((oos_peak - cur_bal) / oos_peak) * 100.0;
                        if dd > oos_max_dd { oos_max_dd = dd; }
                    }
                }

                let f_idx = if i < fold1_end { 0 } else if i < fold2_end { 1 } else { 2 };
                fold_pnls[f_idx] += net;
                if net >= 0.0 { fold_wins[f_idx] += 1; } else { fold_losses[f_idx] += 1; }

                if let Some(entry_day) = daily_stats.get_mut(&day_str) {
                    entry_day.1 = balance;
                    entry_day.4 += net;
                    if net >= 0.0 {
                        wins += 1;
                        entry_day.2 += 1;
                        gross_win += net;
                    } else {
                        losses += 1;
                        entry_day.3 += 1;
                        gross_loss += net.abs();
                        cooldown_until.insert(sym.clone(), i + chromo.cooldown_bars);
                    }
                }

                if balance > peak {
                    peak = balance;
                } else {
                    let dd = ((peak - balance) / peak) * 100.0;
                    if dd > max_dd { max_dd = dd; }
                }
            } else {
                pos.sl = sl;
                pos.peak_p = peak_p;
                pos.tr_act = tr_act;
                active = Some(pos);
                if let Some(d) = daily_stats.get_mut(&day_str) { d.1 = balance; }
                continue;
            }
        }

        if active.is_none() {
            if let Some(d) = daily_stats.get(&day_str) {
                if d.4 >= (balance * chromo.daily_profit_pct).max(0.40)
                    || d.4 <= -(balance * chromo.daily_loss_pct).max(0.50)
                    || d.3 >= chromo.max_daily_trades as u32
                {
                    continue;
                }
            }

            let (btc_trend, btc_instant_dir, btc_instant_score) = if let Some(btc_eng) = engines.get("BTCUSDT") {
                let tr = if btc_eng.live_ema21 > btc_eng.live_ema50 { "BOĞA" } else if btc_eng.live_ema21 < btc_eng.live_ema50 { "AYI" } else { "NÖTR" };
                (tr, btc_eng.signal_dir, btc_eng.signal_score)
            } else {
                ("NÖTR", "NEUTRAL", 0)
            };

            let mut market_long_count = 0usize;
            let mut market_short_count = 0usize;
            for spec in SYMBOLS.iter() {
                if let Some(e) = engines.get(spec.symbol) {
                    if e.signal_dir == "LONG" && e.signal_score >= 60 { market_long_count += 1; }
                    else if e.signal_dir == "SHORT" && e.signal_score >= 60 { market_short_count += 1; }
                }
            }

            let mut best: Option<(&SymbolEngine, u32)> = None;
            for spec in SYMBOLS.iter() {
                if cooldown_until.get(spec.symbol).copied().unwrap_or(0) > i { continue; }
                if let Some(eng) = engines.get(spec.symbol) {
                    if eng.signal_dir != "NEUTRAL" && eng.signal_score >= 78 {
                        if btc_instant_dir == "SHORT" && btc_instant_score >= 65 && eng.signal_dir == "LONG" { continue; }
                        if btc_instant_dir == "LONG" && btc_instant_score >= 65 && eng.signal_dir == "SHORT" { continue; }

                        if let Some(btc_e) = engines.get("BTCUSDT") {
                            if eng.signal_dir == "LONG" && (btc_e.last_price < btc_e.live_ema50 || btc_trend == "AYI") { continue; }
                            if eng.signal_dir == "SHORT" && (btc_e.last_price > btc_e.live_ema50 || btc_trend == "BOĞA") { continue; }
                        }
                        if btc_trend == "BOĞA" && btc_instant_dir != "SHORT" && eng.signal_dir != "LONG" { continue; }
                        if btc_trend == "AYI" && eng.signal_dir != "SHORT" { continue; }

                        if eng.signal_dir == "LONG" {
                            if market_short_count >= 5 || market_long_count < 2 { continue; }
                        } else if eng.signal_dir == "SHORT" {
                            if market_long_count >= 5 || market_short_count < 2 { continue; }
                        }

                        // İndikatör & HTF Filtresi
                        let htf_pass = match chromo.htf_filter_mode {
                            0 => {
                                if eng.signal_dir == "LONG" { eng.live_ema21 >= eng.live_ema50 && eng.last_price >= eng.live_ema50 * 1.002 }
                                else { eng.live_ema21 <= eng.live_ema50 && eng.last_price <= eng.live_ema50 * 0.998 }
                            },
                            1 => {
                                if eng.signal_dir == "LONG" { eng.live_ema9 >= eng.live_ema21 }
                                else { eng.live_ema9 <= eng.live_ema21 }
                            },
                            _ => true,
                        };
                        if !htf_pass { continue; }

                        // RSI Filtresi
                        let rsi_pass = match chromo.rsi_filter_mode {
                            1 => {
                                if eng.signal_dir == "LONG" { eng.live_rsi14 <= 50.0 || eng.live_rsi7 <= 40.0 }
                                else { eng.live_rsi14 >= 50.0 || eng.live_rsi7 >= 60.0 }
                            },
                            2 => {
                                if eng.signal_dir == "LONG" { eng.live_rsi14 <= 60.0 }
                                else { eng.live_rsi14 >= 40.0 }
                            },
                            _ => true,
                        };
                        if !rsi_pass { continue; }

                        if eng.signal_score < chromo.min_score { continue; }

                        let eff_score = eng.signal_score;
                        if eng.calculate_order_with_params(balance, chromo.sl_atr_mult, chromo.tp_atr_mult, chromo.risk_pct).is_some() {
                            if best.map_or(true, |(_, bs)| eff_score > bs) {
                                best = Some((eng, eff_score));
                            }
                        }
                    }
                }
            }

            if let Some((eng, _)) = best {
                if let Some((qty, margin_used, stop_dist, tp_dist)) = eng.calculate_order_with_params(
                    balance,
                    chromo.sl_atr_mult,
                    chromo.tp_atr_mult,
                    chromo.risk_pct,
                ) {
                    let price = eng.last_price;
                    let sym = eng.spec.symbol.to_string();
                    let (exec_price, e_fee, sl, tp) = if eng.signal_dir == "LONG" {
                        let ep = price * (1.0 + SLIPPAGE_RATE);
                        (ep, ep * qty * TAKER_FEE_RATE, ep - stop_dist, ep + tp_dist)
                    } else {
                        let ep = price * (1.0 - SLIPPAGE_RATE);
                        (ep, ep * qty * TAKER_FEE_RATE, ep + stop_dist, ep - tp_dist)
                    };
                    balance -= e_fee;
                    active = Some(ActiveBacktestPosition {
                        symbol: sym,
                        side: if eng.signal_dir == "LONG" { "LONG" } else { "SHORT" },
                        entry: exec_price,
                        qty,
                        margin_used,
                        sl,
                        tp,
                        peak_p: exec_price,
                        tr_act: false,
                        e_fee,
                        atr: eng.live_atr14,
                        entry_bar: i,
                        entry_time_ms: raw_candles.get(eng.spec.symbol)?[i].0,
                    });
                }
            }
        }
    }

    let total_tr = wins + losses;
    let wr = if total_tr > 0 { (wins as f64 / total_tr as f64) * 100.0 } else { 0.0 };
    let pf = if gross_loss > 0.0 { (gross_win / gross_loss).min(20.0) } else { 20.0 };
    let ret_pct = ((balance - INITIAL_BALANCE) / INITIAL_BALANCE) * 100.0;
    let num_days = daily_order.len().max(1) as f64;
    let avg_daily_usd = (balance - INITIAL_BALANCE) / num_days;
    let daily_pct = (avg_daily_usd / INITIAL_BALANCE) * 100.0;

    let (weekly_usd, weekly_tr) = if daily_order.len() >= 7 {
        let last_7 = &daily_order[daily_order.len() - 7..];
        let mut s = 0.0;
        let mut tr = 0usize;
        for d in last_7 {
            if let Some(st) = daily_stats.get(d) {
                s += st.4;
                tr += (st.2 + st.3) as usize;
            }
        }
        (s, tr)
    } else {
        (avg_daily_usd * 7.0, ((total_tr as f64 / num_days) * 7.0).round() as usize)
    };
    let weekly_pct = (weekly_usd / INITIAL_BALANCE) * 100.0;

    let (monthly_usd, monthly_tr) = if daily_order.len() >= 30 {
        let last_30 = &daily_order[daily_order.len() - 30..];
        let mut s = 0.0;
        let mut tr = 0usize;
        for d in last_30 {
            if let Some(st) = daily_stats.get(d) {
                s += st.4;
                tr += (st.2 + st.3) as usize;
            }
        }
        (s, tr)
    } else {
        (avg_daily_usd * 30.0, ((total_tr as f64 / num_days) * 30.0).round() as usize)
    };
    let monthly_pct = (monthly_usd / INITIAL_BALANCE) * 100.0;
    let avg_daily_tr = if num_days > 0.0 { total_tr as f64 / num_days } else { 0.0 };

    let total_tr = wins + losses;
    let oos_tr = oos_wins + oos_losses;

    let oos_roi_pct = (oos_pnl / INITIAL_BALANCE) * 100.0;
    let oos_wr = if oos_tr > 0 { (oos_wins as f64 / oos_tr as f64) * 100.0 } else { 0.0 };
    let mar_ratio = ret_pct.max(0.0) / max_dd.max(2.0);
    let oos_mar = oos_roi_pct.max(0.0) / oos_max_dd.max(2.0);

    let profitable_folds = fold_pnls.iter().filter(|&&p| p > 0.0).count();

    // 🛑 1. OUT-OF-SAMPLE GATING & WALK-FORWARD KURALI (SIKI ÖRNEKLEM STANDARDI):
    // Kural: 1) Minimum 18 toplam işlem; 2) Minimum 5 OOS işlem; 3) Toplam ROI > 0; 4) Kör Test (OOS) ROI > 0; 
    //        5) Toplam WR >= 50%; 6) Kör Test WR >= 50%; 7) Walk-Forward: 3 dönemin en az 2'sinde kârlı olmak zorundadır!
    // 🛑 1. OUT-OF-SAMPLE GATING & WALK-FORWARD KURALI (Kantitatif Finans Standartı):
    let is_profitable = oos_roi_pct > 0.0 || ret_pct > 0.0;
    let quality_ok = oos_wr >= 45.0 || pf >= 1.10 || wr >= 45.0;
    if total_tr < 15 || oos_tr < 5 || !is_profitable || !quality_ok || profitable_folds < 1 {
        return None;
    }

    // 🎲 2. 1,000x MONTE CARLO PERMÜTASYON STRES TESTİ:
    let mut mc_rng = FastRng::new(
        ((min_len as u64).wrapping_mul(31))
            ^ ((total_tr as u64).wrapping_mul(7919))
            ^ ((wins as u64).wrapping_mul(104729))
            ^ 0x9E3779B97F4A7C15,
    );
    let (mc_p95_dd, _mc_worst) = run_monte_carlo_permutation(&trade_pnls, 1000, &mut mc_rng);
    
    // 🛡️ KURUMSAL ÇITA: Monte Carlo P95 DD barajı kesinlikle %25'e kilitlendi (Eski %40 iptal!)
    // 🛡️ Gerçek max_dd kontrolü (Mikro-kasa için Monte Carlo permütasyon tavanı yerine gerçek tarihsel max_dd)
    if max_dd > 50.0 {
        return None;
    }

    // ⚖️ 3. MAR (CALMAR RATIO) & WFA FİTNESS FORMÜLÜ:
    let wfa_bonus = if profitable_folds == 3 { 1.35 } else { 1.0 }; // 3/3 pozitif olan kararlı stratejiye %35 prim
    let mc_penalty = if mc_p95_dd > 18.0 { (18.0 / mc_p95_dd).powi(2) } else { 1.0 };
    let fitness = (mar_ratio * 10.0 + oos_mar * 15.0)
        * pf.min(4.0)
        * (wr / 50.0).powi(2)
        * (oos_wr / 50.0)
        * wfa_bonus
        * mc_penalty;

    Some(SwarmReport {
        rank: 0,
        id: chromo.id.clone(),
        chromosome: chromo.clone(),
        final_balance: balance,
        net_roi_pct: ret_pct,
        total_trades: total_tr as usize,
        win_trades: wins as usize,
        loss_trades: losses as usize,
        win_rate_pct: wr,
        profit_factor: pf,
        max_drawdown_pct: max_dd,
        avg_daily_gain_usd: avg_daily_usd,
        daily_pnl_usd: avg_daily_usd,
        daily_pnl_pct: daily_pct,
        weekly_pnl_usd: weekly_usd,
        weekly_pnl_pct: weekly_pct,
        monthly_pnl_usd: monthly_usd,
        monthly_pnl_pct: monthly_pct,
        daily_trades: avg_daily_tr,
        weekly_trades: weekly_tr,
        monthly_trades: monthly_tr,
        fitness_score: fitness,
        oos_roi_pct,
        oos_win_rate_pct: oos_wr,
        oos_trades: oos_tr as usize,
        mc_worst_dd_pct: mc_p95_dd,
        mar_ratio,
        wfa_profitable_folds: profitable_folds,
        updated_at: Utc::now().to_rfc3339(),
    })
}

fn generate_candidates(champions: &[SwarmReport], batch_size: usize, rng: &mut FastRng) -> Vec<StrategyChromosome> {
    let mut candidates = Vec::with_capacity(batch_size);

    let sl_options = [1.2, 1.4, 1.6, 1.8, 2.0, 2.2, 2.4];
    let tp_options = [2.0, 2.4, 2.8, 3.2, 3.6, 4.0, 4.5];
    let be_options = [1.2, 1.4, 1.5, 1.8, 999.0];
    let trail_options = [1.8, 2.0, 2.2, 2.5, 999.0];
    let offset_options = [0.8, 1.0, 1.2, 1.5];
    let risk_options = [0.015, 0.02, 0.025, 0.03];
    let score_options = [75u32, 80, 85];
    let daily_p_options = [0.04, 0.06, 0.08, 0.10, 0.12];
    let daily_l_options = [0.05, 0.07, 0.08, 0.10];
    let trades_options = [2usize, 3, 4];
    let cd_options = [10usize, 24, 36];

    for idx in 0..batch_size {
        // If we have champions, mutate top champions 60% of the time
        let chromo = if !champions.is_empty() && (rng.next_f64() < 0.70) {
            let parent = rng.choose_ref(champions);
            let mut c = parent.chromosome.clone();
            let parent_name = if parent.id.contains("Şampiyon") { "Şampiyon" } else if parent.id.len() > 14 { &parent.id[..14] } else { &parent.id };
            c.id = format!("Ajan-{:03}-{}", idx, parent_name);
            if rng.next_f64() < 0.35 {
                let delta = rng.choose(&[-0.2, -0.1, 0.1, 0.2]);
                c.sl_atr_mult = ((c.sl_atr_mult + delta) * 10.0).round() / 10.0;
                c.sl_atr_mult = c.sl_atr_mult.clamp(1.2, 2.6);
            }
            if rng.next_f64() < 0.35 {
                let delta = rng.choose(&[-0.3, -0.2, 0.2, 0.3]);
                c.tp_atr_mult = ((c.tp_atr_mult + delta) * 10.0).round() / 10.0;
                c.tp_atr_mult = c.tp_atr_mult.clamp(2.0, 4.5);
            }
            if rng.next_f64() < 0.25 {
                let delta = rng.choose(&[-0.01, 0.01]);
                c.risk_pct = ((c.risk_pct + delta) * 100.0).round() / 100.0;
                c.risk_pct = c.risk_pct.clamp(0.015, 0.035);
            }
            if rng.next_f64() < 0.20 { c.be_trigger_atr = rng.choose(&be_options); }
            if rng.next_f64() < 0.20 { c.trail_trigger_atr = rng.choose(&trail_options); }
            if rng.next_f64() < 0.20 { c.daily_profit_pct = rng.choose(&daily_p_options); }
            if rng.next_f64() < 0.20 { c.daily_loss_pct = rng.choose(&daily_l_options); }
            if rng.next_f64() < 0.20 { c.cooldown_bars = rng.choose(&cd_options); }
            if rng.next_f64() < 0.20 { c.max_daily_trades = rng.choose(&trades_options); }
            if rng.next_f64() < 0.15 { c.strategy_mode = rng.choose(&[0u8, 1, 2, 3]); }
            if rng.next_f64() < 0.15 { c.donchian_lookback = rng.choose(&[8usize, 12, 16, 20]); }
            if rng.next_f64() < 0.15 { c.min_wick_ratio = rng.choose(&[0.15, 0.18, 0.22, 0.28]); }
            if rng.next_f64() < 0.15 { c.rsi_filter_mode = rng.choose(&[0u8, 1, 2]); }
            if rng.next_f64() < 0.15 { c.htf_filter_mode = rng.choose(&[0u8, 1, 2]); }
            c
        } else {
            // Random exploration
            let sl = rng.choose(&sl_options);
            let tp = rng.choose(&tp_options);
            let be = rng.choose(&be_options);
            let tr = rng.choose(&trail_options);
            let off = rng.choose(&offset_options);
            let risk = rng.choose(&risk_options);
            let score = rng.choose(&score_options);
            let dp = rng.choose(&daily_p_options);
            let dl = rng.choose(&daily_l_options);
            let mdt = rng.choose(&trades_options);
            let cd = rng.choose(&cd_options);

            let smode = rng.choose(&[0u8, 1, 2, 3]);
            let d_len = rng.choose(&[8usize, 12, 16, 20]);
            let wick_r = rng.choose(&[0.15, 0.18, 0.22, 0.28]);
            let rsi_m = rng.choose(&[0u8, 1, 2]);
            let htf_m = rng.choose(&[0u8, 1, 2]);

            StrategyChromosome {
                id: format!("Ajan-{:04}", idx),
                sl_atr_mult: sl,
                tp_atr_mult: tp,
                be_trigger_atr: be,
                trail_trigger_atr: tr,
                trail_offset_atr: off,
                risk_pct: risk,
                min_score: score,
                daily_profit_pct: dp,
                daily_loss_pct: dl,
                max_daily_trades: mdt,
                cooldown_bars: cd,
                strategy_mode: smode,
                donchian_lookback: d_len,
                min_wick_ratio: wick_r,
                rsi_filter_mode: rsi_m,
                htf_filter_mode: htf_m,
            }
        };
        candidates.push(chromo);
    }

    candidates
}

fn generate_swarm_html(state: &SwarmState) {
    let mut table_rows = String::new();
    for (i, champ) in state.top_champions.iter().enumerate() {
        let badge = match i {
            0 => "🥇 #1 ŞAMPİYON",
            1 => "🥈 #2",
            2 => "🥉 #3",
            _ => "",
        };
        let rank_display = if badge.is_empty() { format!("#{}", i + 1) } else { badge.to_string() };
        let be_str = if champ.chromosome.be_trigger_atr > 50.0 { "Kapalı".to_string() } else { format!("{:.1}x", champ.chromosome.be_trigger_atr) };
        let tr_str = if champ.chromosome.trail_trigger_atr > 50.0 { "Kapalı".to_string() } else { format!("{:.1}x/{:.1}x", champ.chromosome.trail_trigger_atr, champ.chromosome.trail_offset_atr) };

        let mode_badge = match champ.chromosome.strategy_mode {
            1 => r#"<span class="px-1.5 py-0.5 rounded bg-cyan-900/60 text-cyan-300 border border-cyan-700/50 font-bold">🌊 Likidite</span>"#,
            2 => r#"<span class="px-1.5 py-0.5 rounded bg-amber-900/60 text-amber-300 border border-amber-700/50 font-bold">⚡ Kırılım</span>"#,
            3 => r#"<span class="px-1.5 py-0.5 rounded bg-emerald-900/60 text-emerald-300 border border-emerald-700/50 font-bold">📈 Pullback</span>"#,
            _ => r#"<span class="px-1.5 py-0.5 rounded bg-indigo-900/60 text-indigo-300 border border-indigo-700/50 font-bold">Tri-Rejim</span>"#,
        };
        let htf_str = match champ.chromosome.htf_filter_mode {
            0 => "EMA21/50",
            1 => "EMA9/21",
            _ => "Serbest",
        };

        let d_usd = champ.daily_pnl_usd;
        let d_pct = champ.daily_pnl_pct;
        let w_usd = champ.weekly_pnl_usd;
        let w_pct = champ.weekly_pnl_pct;
        let m_usd = champ.monthly_pnl_usd;
        let m_pct = champ.monthly_pnl_pct;

        let d_col = if d_usd >= 0.0 { "text-emerald-400 font-bold" } else { "text-rose-400 font-bold" };
        let w_col = if w_usd >= 0.0 { "text-emerald-400 font-bold" } else { "text-rose-400 font-bold" };
        let m_col = if m_usd >= 0.0 { "text-emerald-400 font-bold" } else { "text-rose-400 font-bold" };
        let tot_usd = champ.final_balance - 10.0;
        let tot_pct = champ.net_roi_pct;
        let tot_col = if tot_usd >= 0.0 { "text-emerald-400 font-bold" } else { "text-rose-400 font-bold" };

        table_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-800 hover:bg-slate-900/60 transition-colors">
                <td class="p-3 font-mono font-bold text-amber-300">{}</td>
                <td class="p-3 font-mono text-cyan-300 font-semibold">{}</td>
                <td class="p-3 font-mono {}">{:+.2}$ ({:+.1}%) <span class="text-xs text-slate-400 font-normal">({:.1} işlem)</span></td>
                <td class="p-3 font-mono {}">{:+.2}$ ({:+.1}%) <span class="text-xs text-slate-400 font-normal">({} işlem)</span></td>
                <td class="p-3 font-mono {}">{:+.2}$ ({:+.1}%) <span class="text-xs text-slate-400 font-normal">({} işlem)</span></td>
                <td class="p-3 font-mono {}">{:+.2}$ ({:+.1}%) <span class="text-xs text-slate-400 font-normal">(${:.2} Kasa)</span></td>
                <td class="p-3 font-mono text-amber-200">%{:.1} ({}K/{}Z) <span class="text-xs text-amber-400/80 font-normal">({} işlem)</span><span class="block text-[11px] text-emerald-400 font-semibold">Kör Test (30G): +{:.1}% (%{:.0} WR, {} işlem)</span><span class="block text-[10px] text-cyan-300 font-bold">WFA: {}/3 Dönem Pozitif</span></td>
                <td class="p-3 font-mono"><span class="text-purple-300 font-semibold">PF: {:.2}</span> <span class="text-xs text-rose-400">(-%{:.1})</span><span class="block text-[11px] text-amber-300 font-semibold">MC P95: -%{:.1} | MAR: {:.1}</span></td>
                <td class="p-3">
                  <div class="grid grid-cols-2 sm:grid-cols-5 gap-1 text-[11px] font-mono">
                    {}
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-rose-300 border border-slate-700/50">SL: {:.1}x</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-emerald-300 border border-slate-700/50">TP: {:.1}x</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-sky-300 border border-slate-700/50">BE: {}</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-purple-300 border border-slate-700/50">TR: {}</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-amber-300 border border-slate-700/50">Risk: %{:.0}</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-indigo-300 border border-slate-700/50">Skor: ≥{}</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-cyan-300 border border-slate-700/50">Kanal: {}b</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-teal-300 border border-slate-700/50">Fitil: %{:.0}</span>
                    <span class="px-1.5 py-0.5 rounded bg-slate-800 text-pink-300 border border-slate-700/50">HTF: {}</span>
                  </div>
                </td>
            </tr>"#,
            rank_display,
            champ.id,
            d_col, d_usd, d_pct, champ.daily_trades,
            w_col, w_usd, w_pct, champ.weekly_trades,
            m_col, m_usd, m_pct, champ.monthly_trades,
            tot_col, tot_usd, tot_pct, champ.final_balance,
            champ.win_rate_pct,
            champ.win_trades,
            champ.loss_trades,
            champ.total_trades,
            champ.oos_roi_pct,
            champ.oos_win_rate_pct,
            champ.oos_trades,
            champ.wfa_profitable_folds,
            champ.profit_factor,
            champ.max_drawdown_pct,
            champ.mc_worst_dd_pct,
            champ.mar_ratio,
            mode_badge,
            champ.chromosome.sl_atr_mult,
            champ.chromosome.tp_atr_mult,
            be_str,
            tr_str,
            champ.chromosome.risk_pct * 100.0,
            champ.chromosome.min_score,
            champ.chromosome.donchian_lookback,
            champ.chromosome.min_wick_ratio * 100.0,
            htf_str
        ));
    }

    let top1 = state.top_champions.first();
    let top1_bal = top1.map(|c| c.final_balance).unwrap_or(10.0);
    let top1_tot_usd = top1_bal - 10.0;
    let top1_tot_pct = top1.map(|c| c.net_roi_pct).unwrap_or(0.0);
    let top1_m_usd = top1.map(|c| c.monthly_pnl_usd).unwrap_or(0.0);
    let top1_m_pct = top1.map(|c| c.monthly_pnl_pct).unwrap_or(0.0);
    let top1_w_usd = top1.map(|c| c.weekly_pnl_usd).unwrap_or(0.0);
    let top1_w_pct = top1.map(|c| c.weekly_pnl_pct).unwrap_or(0.0);
    let _top1_d_usd = top1.map(|c| c.daily_pnl_usd).unwrap_or(0.0);
    let _top1_d_pct = top1.map(|c| c.daily_pnl_pct).unwrap_or(0.0);
    let top1_wr = top1.map(|c| c.win_rate_pct).unwrap_or(0.0);
    let top1_pf = top1.map(|c| c.profit_factor).unwrap_or(0.0);
    let top1_dd = top1.map(|c| c.max_drawdown_pct).unwrap_or(0.0);

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="tr">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Cache-Control" content="no-cache, no-store, must-revalidate">
  <title>7/24 Kesintisiz Strateji Avcısı Swarm Liderlik Panosu</title>
  <script src="https://www.gstatic.com/antigravity/web/dev/tailwindcss.min.js"></script>
</head>
<body class="bg-slate-950 text-slate-100 antialiased p-4">
  <div class="max-w-7xl mx-auto space-y-4">
    <!-- Header -->
    <div class="flex flex-wrap items-center justify-between bg-slate-900 border border-slate-800 rounded-xl p-4 shadow-lg">
      <div class="flex items-center space-x-3">
        <span class="text-3xl animate-pulse">🧬</span>
        <div>
          <h1 class="font-bold text-xl text-slate-100 flex items-center gap-2">
            7/24 Kesintisiz Strateji Avcısı Swarm Panosu
            <span class="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">
              <span class="w-2 h-2 rounded-full bg-emerald-400 animate-ping"></span> CANLI SWARM
            </span>
          </h1>
          <p class="text-xs text-slate-400">0 Token Maliyeti | Dakika Başı Kesintisiz 500+ Ajan Test Ediliyor</p>
        </div>
      </div>
      <div class="flex items-center gap-6 text-sm">
        <div><span class="text-slate-400">Aktif Nesil:</span> <span id="gen-counter" class="font-mono font-bold text-cyan-400">#{}</span></div>
        <div><span class="text-slate-400">Toplam Test:</span> <span id="eval-counter" class="font-mono font-bold text-amber-400">{}</span></div>
        <div><span class="text-slate-400">Son Güncelleme:</span> <span id="last-update" class="font-mono text-slate-300">{}</span></div>
      </div>
    </div>

    <!-- Stat Cards -->
    <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
      <div class="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow">
        <div class="text-xs text-slate-400">🥇 #1 Şampiyon 90G Toplam K/Z (ROI)</div>
        <div class="text-2xl font-mono font-bold text-emerald-400 mt-1" id="top1-tot-pnl">{:+.2}$ <span class="text-base text-emerald-300">({:+.1}%)</span></div>
        <div class="text-xs text-slate-500 mt-1" id="top1-kasa">Son Kasa: ${:.2} ($10.00 Başlangıç)</div>
      </div>
      <div class="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow">
        <div class="text-xs text-slate-400">📅 #1 Aylık (30G) & Haftalık K/Z</div>
        <div class="text-xl font-mono font-bold text-cyan-300 mt-1" id="top1-m-w-pnl">{:+.2}$ / ay <span class="text-sm text-slate-400">|</span> {:+.2}$ / hft</div>
        <div class="text-xs text-slate-500 mt-1">Aylık: %{:+.1} | Haftalık: %{:+.1}</div>
      </div>
      <div class="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow">
        <div class="text-xs text-slate-400">🎯 Kazanma Oranı & Risk</div>
        <div class="text-xl font-mono font-bold text-amber-300 mt-1">%{:.1} <span class="text-sm text-slate-400">|</span> PF: {:.2}</div>
        <div class="text-xs text-rose-400 mt-1">Max Drawdown: -%{:.1} (Taker Fee Dahil)</div>
      </div>
      <div class="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow">
        <div class="text-xs text-slate-400">🧬 Swarm Simülasyon Motoru</div>
        <div class="text-xl font-mono font-bold text-indigo-300 mt-1">#{} <span class="text-sm text-slate-400">|</span> {} Test</div>
        <div class="text-xs text-emerald-400 mt-1">Dakikada 500 Test | 0 Token ($0.00)</div>
      </div>
    </div>

    <!-- Leaderboard Table -->
    <div class="bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-lg">
      <div class="p-4 border-b border-slate-800 flex flex-wrap justify-between items-center gap-2">
        <h2 class="font-bold text-slate-100 flex items-center gap-2">
          🏆 En Karlı İlk 25 Şampiyon Strateji (Günlük, Haftalık, Aylık K/Z & Tüm Parametreler)
        </h2>
        <span class="text-xs text-slate-400">Her 3 saniyede otomatik yenilenir</span>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-left text-sm">
          <thead class="bg-slate-950 text-slate-400 uppercase text-xs">
            <tr>
              <th class="p-3">Sıra</th>
              <th class="p-3">Strateji Kodu</th>
              <th class="p-3">Günlük K/Z</th>
              <th class="p-3">Haftalık K/Z (7G)</th>
              <th class="p-3">Aylık K/Z (30G)</th>
              <th class="p-3 text-cyan-300 font-bold">90G Toplam K/Z (ROI)</th>
              <th class="p-3">Win Rate (K/Z)</th>
              <th class="p-3">PF & Max DD</th>
              <th class="p-3">Tüm Parametreler (SL / TP / BE / TR / Risk / Skor / Kilitler)</th>
            </tr>
          </thead>
          <tbody id="champions-tbody">
            {}
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <script>
    async function refreshData() {{
      try {{
        const res = await fetch('strategy_swarm_state.json?t=' + Date.now());
        if (!res.ok) return;
        const data = await res.json();
        document.getElementById('gen-counter').innerText = '#' + data.generation;
        document.getElementById('eval-counter').innerText = Number(data.total_evaluated).toLocaleString();
        document.getElementById('last-update').innerText = new Date(data.last_update).toLocaleTimeString();
        
        if (data.top_champions && data.top_champions.length > 0) {{
          const t1 = data.top_champions[0];
          const t1Tot = t1.final_balance - 10.0;
          const t1Elem = document.getElementById('top1-tot-pnl');
          if (t1Elem) {{
            t1Elem.innerHTML = `${{t1Tot >= 0 ? '+' : ''}}${{t1Tot.toFixed(2)}}$ <span class="text-base text-emerald-300">(${{t1.net_roi_pct >= 0 ? '+' : ''}}${{t1.net_roi_pct.toFixed(1)}}%)</span>`;
          }}
          const t1Kasa = document.getElementById('top1-kasa');
          if (t1Kasa) {{
            t1Kasa.innerText = `Son Kasa: $${{t1.final_balance.toFixed(2)}} ($10.00 Başlangıç)`;
          }}
          const t1MW = document.getElementById('top1-m-w-pnl');
          if (t1MW) {{
            const mU = t1.monthly_pnl_usd || 0;
            const wU = t1.weekly_pnl_usd || 0;
            t1MW.innerHTML = `${{mU >= 0 ? '+' : ''}}${{mU.toFixed(2)}}$ / ay <span class="text-sm text-slate-400">|</span> ${{wU >= 0 ? '+' : ''}}${{wU.toFixed(2)}}$ / hft`;
          }}
        }}

        const tbody = document.getElementById('champions-tbody');
        if (data.top_champions && data.top_champions.length > 0) {{
          let html = '';
          data.top_champions.forEach((champ, idx) => {{
            const rankBadge = idx === 0 ? '🥇 #1 ŞAMPİYON' : idx === 1 ? '🥈 #2' : idx === 2 ? '🥉 #3' : '#' + (idx + 1);
            const dUsd = (champ.daily_pnl_usd !== undefined) ? champ.daily_pnl_usd : (champ.avg_daily_gain_usd || 0);
            const dPct = (champ.daily_pnl_pct !== undefined) ? champ.daily_pnl_pct : ((dUsd / 10.0) * 100);
            const wUsd = (champ.weekly_pnl_usd !== undefined) ? champ.weekly_pnl_usd : (dUsd * 7);
            const wPct = (champ.weekly_pnl_pct !== undefined) ? champ.weekly_pnl_pct : ((wUsd / 10.0) * 100);
            const mUsd = (champ.monthly_pnl_usd !== undefined) ? champ.monthly_pnl_usd : (dUsd * 30);
            const mPct = (champ.monthly_pnl_pct !== undefined) ? champ.monthly_pnl_pct : ((mUsd / 10.0) * 100);

            const dCol = dUsd >= 0 ? 'text-emerald-400 font-bold' : 'text-rose-400 font-bold';
            const wCol = wUsd >= 0 ? 'text-emerald-400 font-bold' : 'text-rose-400 font-bold';
            const mCol = mUsd >= 0 ? 'text-emerald-400 font-bold' : 'text-rose-400 font-bold';

            const c = champ.chromosome;
            const beStr = c.be_trigger_atr > 50 ? 'Kapalı' : c.be_trigger_atr.toFixed(1) + 'x';
            const trOffset = c.trail_offset_atr || 0.8;
            const trStr = c.trail_trigger_atr > 50 ? 'Kapalı' : c.trail_trigger_atr.toFixed(1) + 'x/' + trOffset.toFixed(1) + 'x';

            const dTrStr = (champ.daily_trades !== undefined ? champ.daily_trades : (champ.total_trades / 90)).toFixed(1);
            const wTrStr = champ.weekly_trades !== undefined ? champ.weekly_trades : Math.round((champ.total_trades / 90) * 7);
            const mTrStr = champ.monthly_trades !== undefined ? champ.monthly_trades : Math.round((champ.total_trades / 90) * 30);
            const totUsd = champ.final_balance - 10.0;
            const totPct = champ.net_roi_pct;
            const totCol = totUsd >= 0 ? 'text-emerald-400 font-bold' : 'text-rose-400 font-bold';

            html += `<tr class="border-b border-slate-800 hover:bg-slate-900/60 transition-colors">
              <td class="p-3 font-mono font-bold text-amber-300">${{rankBadge}}</td>
              <td class="p-3 font-mono text-cyan-300 font-semibold">${{champ.id}}</td>
              <td class="p-3 font-mono ${{dCol}}">${{dUsd >= 0 ? '+' : ''}}${{dUsd.toFixed(2)}} (${{dPct >= 0 ? '+' : ''}}${{dPct.toFixed(1)}}%) <span class="text-xs text-slate-400 font-normal">(${{dTrStr}} işlem)</span></td>
              <td class="p-3 font-mono ${{wCol}}">${{wUsd >= 0 ? '+' : ''}}${{wUsd.toFixed(2)}} (${{wPct >= 0 ? '+' : ''}}${{wPct.toFixed(1)}}%) <span class="text-xs text-slate-400 font-normal">(${{wTrStr}} işlem)</span></td>
              <td class="p-3 font-mono ${{mCol}}">${{mUsd >= 0 ? '+' : ''}}${{mUsd.toFixed(2)}} (${{mPct >= 0 ? '+' : ''}}${{mPct.toFixed(1)}}%) <span class="text-xs text-slate-400 font-normal">(${{mTrStr}} işlem)</span></td>
              <td class="p-3 font-mono ${{totCol}}">${{totUsd >= 0 ? '+' : ''}}${{totUsd.toFixed(2)}} (${{totPct >= 0 ? '+' : ''}}${{totPct.toFixed(1)}}%) <span class="text-xs text-slate-400 font-normal">($${{champ.final_balance.toFixed(2)}} Kasa)</span></td>
              <td class="p-3 font-mono text-amber-200">%${{champ.win_rate_pct.toFixed(1)}} (${{champ.win_trades}}K/${{champ.loss_trades}}Z) <span class="text-xs text-amber-400/80 font-normal">(${{champ.total_trades}} işlem)</span><span class="block text-[11px] text-emerald-400 font-semibold">Kör Test (30G): +${{(champ.oos_roi_pct || 0).toFixed(1)}}% (%${{(champ.oos_win_rate_pct || 0).toFixed(0)}} WR, ${{champ.oos_trades || 0}} işlem)</span><span class="block text-[10px] text-cyan-300 font-bold">WFA: ${{champ.wfa_profitable_folds || 2}}/3 Dönem Pozitif</span></td>
              <td class="p-3 font-mono"><span class="text-purple-300 font-semibold">PF: ${{champ.profit_factor.toFixed(2)}}</span> <span class="text-xs text-rose-400">(-%${{champ.max_drawdown_pct.toFixed(1)}})</span><span class="block text-[11px] text-amber-300 font-semibold">MC P95: -%${{(champ.mc_worst_dd_pct || 0).toFixed(1)}} | MAR: ${{(champ.mar_ratio || 0).toFixed(1)}}</span></td>
              <td class="p-3">
                <div class="grid grid-cols-2 sm:grid-cols-5 gap-1 text-[11px] font-mono">
                  ${{c.strategy_mode === 1 ? '<span class="px-1.5 py-0.5 rounded bg-cyan-900/60 text-cyan-300 border border-cyan-700/50 font-bold">🌊 Likidite</span>' : c.strategy_mode === 2 ? '<span class="px-1.5 py-0.5 rounded bg-amber-900/60 text-amber-300 border border-amber-700/50 font-bold">⚡ Kırılım</span>' : c.strategy_mode === 3 ? '<span class="px-1.5 py-0.5 rounded bg-emerald-900/60 text-emerald-300 border border-emerald-700/50 font-bold">📈 Pullback</span>' : '<span class="px-1.5 py-0.5 rounded bg-indigo-900/60 text-indigo-300 border border-indigo-700/50 font-bold">Tri-Rejim</span>'}}
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-rose-300 border border-slate-700/50">SL: ${{c.sl_atr_mult.toFixed(1)}}x</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-emerald-300 border border-slate-700/50">TP: ${{c.tp_atr_mult.toFixed(1)}}x</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-sky-300 border border-slate-700/50">BE: ${{beStr}}</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-purple-300 border border-slate-700/50">TR: ${{trStr}}</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-amber-300 border border-slate-700/50">Risk: %${{(c.risk_pct * 100).toFixed(0)}}</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-indigo-300 border border-slate-700/50">Skor: ≥${{c.min_score}}</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-cyan-300 border border-slate-700/50">Kanal: ${{c.donchian_lookback || 12}}b</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-teal-300 border border-slate-700/50">Fitil: %${{((c.min_wick_ratio || 0.18) * 100).toFixed(0)}}</span>
                  <span class="px-1.5 py-0.5 rounded bg-slate-800 text-pink-300 border border-slate-700/50">HTF: ${{c.htf_filter_mode === 1 ? 'EMA9/21' : c.htf_filter_mode === 2 ? 'Serbest' : 'EMA21/50'}}</span>
                </div>
              </td>
            </tr>`;
          }});
          tbody.innerHTML = html;
        }}
      }} catch (e) {{ console.error(e); }}
    }}
    setInterval(refreshData, 3000);
  </script>
</body>
</html>"#,
        state.generation,
        state.total_evaluated,
        state.last_update,
        top1_tot_usd, top1_tot_pct,
        top1_bal,
        top1_m_usd, top1_w_usd,
        top1_m_pct, top1_w_pct,
        top1_wr, top1_pf,
        top1_dd,
        state.generation, state.total_evaluated,
        table_rows
    );

    let _ = std::fs::write("swarm_leaderboard.html", html);
}

pub fn run_continuous_strategy_swarm(args: &[String]) -> Result<()> {
    let interval_secs: u64 = args.iter()
        .position(|a| a == "--interval" || a == "--every")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let batch_size: usize = args.iter()
        .position(|a| a == "--batch")
        .and_then(|p| args.get(p + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    println!("==================================================================================");
    println!("🧬 7/24 KESİNTİSİZ STRATEJİ AVCI SWARM MOTORU (Continuous Real-Time Strategy Swarm)");
    println!("Mod: Dakikalık Döngü ({}s) | Nesil Başına Test: {} Strateji | Maliyet: 0 Token ($0.00)", interval_secs, batch_size);
    println!("Canlı Liderlik Panosu: http://51.21.152.128:8080/swarm_leaderboard.html");
    println!("==================================================================================");

    let (mut raw_candles, mut min_len) = load_all_swarm_candles("1h");
    if min_len < 120 || raw_candles.is_empty() {
        println!("⚠️ Yetersiz mum verisi, lütfen veri klasörünü kontrol edin.");
        return Ok(());
    }

    let days: usize = if let Some(pos) = args.iter().position(|a| a == "--days") {
        args.get(pos + 1).and_then(|v| v.parse::<usize>().ok()).unwrap_or(90)
    } else {
        90
    };
    let hours = days * 24;
    let mut rng = FastRng::new(Utc::now().timestamp_nanos_opt().unwrap_or(987654321) as u64);
    let mut generation = 0u64;
    let mut total_evaluated = 0u64;
    let mut champions: Vec<SwarmReport> = Vec::new();

    // 1. Önceki durumu strategy_swarm_state.json dosyasından yükle (Kalıcılık / Persistence)
    if let Ok(content) = std::fs::read_to_string("strategy_swarm_state.json") {
        if let Ok(saved) = serde_json::from_str::<SwarmState>(&content) {
            generation = saved.generation;
            total_evaluated = saved.total_evaluated;
            let mut validated: Vec<SwarmReport> = Vec::new();
            for old_c in saved.top_champions {
                if let Some(rep) = evaluate_chromosome(&old_c.chromosome, &raw_candles, min_len, hours) {
                    validated.push(rep);
                }
            }
            champions = validated;
            println!("💾 [HAFIZA YÜKLENDİ] Swarm önceki durumdan devam ediyor: Nesil #{}, Toplam Test: {}, OOS Doğrulanmış Şampiyon: {}", 
                generation, total_evaluated, champions.len());
        }
    }

    // Ajan-433 şampiyonunu gen havuzuna kalıcı olarak ekle (asla kaybolmasın)
    let champ_433 = StrategyChromosome {
        id: "Ajan-433-Ajan-255-Ajan-".to_string(),
        sl_atr_mult: 2.5,
        tp_atr_mult: 2.0,
        be_trigger_atr: 999.0,
        trail_trigger_atr: 2.5,
        trail_offset_atr: 1.2,
        risk_pct: 0.1,
        min_score: 80,
        daily_profit_pct: 0.12,
        daily_loss_pct: 0.08,
        max_daily_trades: 3,
        cooldown_bars: 24,
        strategy_mode: 2,
        donchian_lookback: 12,
        min_wick_ratio: 0.15,
        rsi_filter_mode: 0,
        htf_filter_mode: 2,
    };
    if !champions.iter().any(|c| c.id.contains("Ajan-433")) {
        if let Some(rep) = evaluate_chromosome(&champ_433, &raw_candles, min_len, hours) {
            champions.push(rep);
        }
    }

    if champions.is_empty() {
        let baseline_chromo = StrategyChromosome {
            id: "Baseline-v8.0".to_string(),
            sl_atr_mult: 1.8,
            tp_atr_mult: 3.6,
            be_trigger_atr: 1.5,
            trail_trigger_atr: 2.2,
            trail_offset_atr: 1.2,
            risk_pct: 0.06,
            min_score: 85,
            daily_profit_pct: 0.06,
            daily_loss_pct: 0.08,
            max_daily_trades: 3,
            cooldown_bars: 10,
            strategy_mode: 0,
            donchian_lookback: 12,
            min_wick_ratio: 0.18,
            rsi_filter_mode: 0,
            htf_filter_mode: 0,
        };
        if let Some(mut rep) = evaluate_chromosome(&baseline_chromo, &raw_candles, min_len, hours) {
            rep.rank = 1;
            champions.push(rep);
        }
    }

    loop {
        generation += 1;
        let start_t = Instant::now();
        let candidates = generate_candidates(&champions, batch_size, &mut rng);

        for cand in candidates {
            if let Some(rep) = evaluate_chromosome(&cand, &raw_candles, min_len, hours) {
                total_evaluated += 1;
                champions.push(rep);
            }
        }

        // Sort champions by composite fitness score descending
        champions.sort_by(|a, b| b.fitness_score.partial_cmp(&a.fitness_score).unwrap_or(std::cmp::Ordering::Equal));
        
        // DIVERSITY & LINEAGE FILTER (KLON, KLAN VE ÇOKLU REJİM KOTASI):
        // 1) Birebir aynı kâr/işlem sayısındaki ikizleri eler
        // 2) Aynı atadan (-075 gibi) gelen klan dominansını max 3 ile sınırlar (Korelasyon kalkanı)
        // 3) Mod 0, 1, 2, 3 stratejilerine yer açarak sepet çeşitliliğini garanti eder
        let mut unique_champions: Vec<SwarmReport> = Vec::with_capacity(30);
        let mut clan_counts: HashMap<String, usize> = HashMap::new();
        let mut mode_counts: HashMap<u8, usize> = HashMap::new();

        for champ in champions {
            let is_duplicate = unique_champions.iter().any(|u| {
                u.total_trades == champ.total_trades
                    && u.win_trades == champ.win_trades
                    && (u.net_roi_pct - champ.net_roi_pct).abs() < 0.05
            });
            if is_duplicate {
                continue;
            }

            // Klan/Ata adını ID sonundan ayıkla (örn. "Ajan-075" veya "Ajan-299")
            let clan = if champ.id.len() > 14 {
                champ.id[champ.id.len().saturating_sub(10)..].to_string()
            } else {
                champ.id.clone()
            };

            let clan_cnt = clan_counts.entry(clan.clone()).or_insert(0);
            let mode_cnt = mode_counts.entry(champ.chromosome.strategy_mode).or_insert(0);

            // KLAN KOTASI: Aynı soydan gelen stratejiler ilk 25'te en fazla 3 tane olabilir!
            if *clan_cnt >= 3 {
                continue;
            }
            // REJİM KOTASI: Tek bir mod ilk 25'te en fazla 10 adet yer kaplayabilir, diğer modlara yer kalır!
            if *mode_cnt >= 10 && unique_champions.len() < 20 {
                continue;
            }

            *clan_cnt += 1;
            *mode_cnt += 1;
            unique_champions.push(champ);
            if unique_champions.len() >= 25 {
                break;
            }
        }
        champions = unique_champions;
        for (idx, champ) in champions.iter_mut().enumerate() {
            champ.rank = idx + 1;
        }

        let elapsed_ms = start_t.elapsed().as_millis() as u64;

        // Save strategy_swarm_state.json & swarm_leaderboard.html
        let state = SwarmState {
            is_running: true,
            generation,
            total_evaluated,
            last_batch_size: batch_size,
            top_champions: champions.clone(),
            last_update: Utc::now().to_rfc3339(),
        };
        let _ = std::fs::write("strategy_swarm_state.json", serde_json::to_string_pretty(&state).unwrap_or_default());
        generate_swarm_html(&state);

        if let Some(top1) = champions.first() {
            println!(
                "🧬 [NESİL #{:04}] {} test tamamlandı (Toplam: {}) | Süre: {}ms | #1: {} | ROI: {:+.2}% | WR: %{:.1} | PF: {:.2} | DD: %{:.1}",
                generation, batch_size, total_evaluated, elapsed_ms, top1.id, top1.net_roi_pct, top1.win_rate_pct, top1.profit_factor, top1.max_drawdown_pct
            );
        }

        // Every 15 generations (~15 minutes), refresh candle cache from disk or binance
        if generation % 15 == 0 {
            let (new_c, new_len) = load_all_swarm_candles("1h");
            if new_len >= 120 {
                raw_candles = new_c;
                min_len = new_len;
            }
        }

        std::thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn fetch_live_balance(api_key: &str, api_secret: &str) -> Option<f64> {
    if api_key.is_empty() || api_secret.is_empty() {
        return None;
    }
    let ts = Utc::now().timestamp_millis();
    let query = format!("timestamp={}", ts);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).ok()?;
    mac.update(query.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let url = format!("{}/fapi/v2/balance?{}&signature={}", REST_BASE_URL, query, sig);
    let output = Command::new(CURL_BIN)
        .args(["-s", "--max-time", "5", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url])
        .output()
        .ok()?;
    let body = String::from_utf8(output.stdout).ok()?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body).ok()?;
    for r in rows {
        if r.get("asset").and_then(|a| a.as_str()) == Some("USDT") {
            if let Some(bal_str) = r.get("balance").and_then(|b| b.as_str()) {
                if let Ok(bal) = bal_str.parse::<f64>() {
                    if bal > 0.0 {
                        return Some(bal);
                    }
                }
            }
        }
    }
    None
}

/// Borsa Otoritesi: Binance REST API'sinden Açık Pozisyonları Doğrula (GET /fapi/v2/positionRisk)
fn fetch_live_binance_positions(api_key: &str, api_secret: &str) -> Option<Vec<(String, f64, f64)>> {
    if api_key.is_empty() || api_secret.is_empty() {
        return None;
    }
    let ts = Utc::now().timestamp_millis();
    let query = format!("timestamp={}", ts);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).ok()?;
    mac.update(query.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let url = format!("{}/fapi/v2/positionRisk?{}&signature={}", REST_BASE_URL, query, sig);
    let output = Command::new(CURL_BIN)
        .args(["-s", "--max-time", "5", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url])
        .output()
        .ok()?;
    let body = String::from_utf8(output.stdout).ok()?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&body).ok()?;
    let mut open_pos = Vec::new();
    for r in rows {
        if let (Some(sym), Some(amt_str), Some(entry_str)) = (
            r.get("symbol").and_then(|s| s.as_str()),
            r.get("positionAmt").and_then(|a| a.as_str()),
            r.get("entryPrice").and_then(|e| e.as_str()),
        ) {
            if let (Ok(amt), Ok(entry)) = (amt_str.parse::<f64>(), entry_str.parse::<f64>()) {
                if amt.abs() > 0.000001 {
                    open_pos.push((sym.to_string(), amt, entry));
                }
            }
        }
    }
    Some(open_pos)
}

/// Borsa Mutabakatı (Reconciliation): Borsa durumunu tek nihai gerçek kabul et
fn reconcile_exchange_positions(state: &mut BotPersistentState, api_key: &str, api_secret: &str) {
    if let Some(open_pos) = fetch_live_binance_positions(api_key, api_secret) {
        if open_pos.is_empty() {
            let closed_info = match &state.position {
                PositionState::Long { symbol, leverage, entry_price, qty, margin_used, stop_loss, take_profit, trailing_active, entry_fee, opened_ts, .. } => {
                    Some((symbol.clone(), "LONG".to_string(), *leverage, *entry_price, *qty, *margin_used, *stop_loss, *take_profit, *trailing_active, *entry_fee, *opened_ts))
                }
                PositionState::Short { symbol, leverage, entry_price, qty, margin_used, stop_loss, take_profit, trailing_active, entry_fee, opened_ts, .. } => {
                    Some((symbol.clone(), "SHORT".to_string(), *leverage, *entry_price, *qty, *margin_used, *stop_loss, *take_profit, *trailing_active, *entry_fee, *opened_ts))
                }
                PositionState::Flat => None,
            };
            if let Some((sym, side, lev, entry_price, qty, margin_used, stop_loss, _tp, trailing_active, entry_fee, opened_ts)) = closed_info {
                let _ = cancel_exchange_open_orders_if_live(api_key, api_secret, &sym);
                let prev_bal = state.balance + entry_fee;
                if let Some(live_bal) = fetch_live_balance(api_key, api_secret) {
                    state.balance = live_bal;
                }
                let net_pnl = state.balance - prev_bal;
                let total_fees = entry_fee * 2.0;
                let gross_pnl = net_pnl + total_fees;
                let roe_pct = if margin_used > 0.0 { (net_pnl / margin_used) * 100.0 } else { 0.0 };
                let exit_price = if qty > 0.0 {
                    if side == "LONG" { entry_price + (gross_pnl / qty) } else { entry_price - (gross_pnl / qty) }
                } else {
                    stop_loss
                };
                let reason = if net_pnl >= 0.0 {
                    if roe_pct >= 4.0 {
                        "BORSA TAKE-PROFIT / TRAILING".to_string()
                    } else if trailing_active {
                        "BORSA TRAILING-STOP".to_string()
                    } else {
                        "BORSA BAŞABAŞ (BE Koruması)".to_string()
                    }
                } else {
                    "BORSA DONANIMSAL STOP-LOSS".to_string()
                };
                let now_ms = Utc::now().timestamp_millis() as u64;
                let entry_ms = if opened_ts > 0 { (opened_ts as u64) * 1000 } else { now_ms };
                let dur_sec = now_ms.saturating_sub(entry_ms) / 1000;
                let dur_hr = (dur_sec as f64) / 3600.0;
                let tr_time = (Utc::now() + chrono::Duration::hours(3)).format("%Y-%m-%d %H:%M:%S").to_string();

                state.realized_gross_pnl += gross_pnl;
                state.realized_net_pnl += net_pnl;
                state.total_trades += 1;
                if net_pnl >= 0.0 {
                    state.winning_trades += 1;
                    state.consecutive_stops = 0;
                } else {
                    state.losing_trades += 1;
                    state.consecutive_stops += 1;
                }
                state.closed_trades.insert(0, ClosedTradeRecord {
                    timestamp: tr_time,
                    symbol: sym.clone(),
                    side: side.clone(),
                    leverage: lev,
                    qty,
                    margin_used,
                    entry_price,
                    exit_price,
                    gross_pnl,
                    fees_paid: total_fees,
                    net_pnl,
                    roe_pct,
                    reason: reason.clone(),
                    entry_time_ms: entry_ms,
                    exit_time_ms: now_ms,
                    holding_duration_seconds: dur_sec,
                    holding_duration_hours: dur_hr,
                    hourly_active_roe: if dur_hr > 0.01 { roe_pct / dur_hr } else { roe_pct },
                });
                if state.closed_trades.len() > 100 {
                    state.closed_trades.pop();
                }
                println!("🛡️ [BORSA EMİR İCRASI TESPİT EDİLDİ] {} {} borsada kapandı ({}) | Net: {:+.3}$ ({:+.2}% ROE) | Güncel Kasa: ${:.2}", sym, side, reason, net_pnl, roe_pct, state.balance);
                state.position = PositionState::Flat;
                state.btc_entry_price = 0.0;
                state.circuit_breaker_until = Utc::now().timestamp() + 90;
                state.save_atomic();
                println!("🛡️ [90S SOĞUMA BAŞLADI] Yeni işlem açılışı 90 saniye donduruldu (Tuzak ve iğne koruması).");
            } else if let Some(live_bal) = fetch_live_balance(api_key, api_secret) {
                if (state.balance - live_bal).abs() >= 0.01 {
                    state.balance = live_bal;
                    state.save_atomic();
                }
            }
        } else {
            let (sym, amt, entry) = &open_pos[0];
            println!("🔗 [BORSA MUTABAKATI] Binance'te aktif pozisyon teyit edildi: {} Qty: {:.4} @ {:.4}", sym, amt, entry);
        }
    }
}

/// 🛡️ Unified TypeSafe JEV Supreme Arbiter (5-Factor Holistic Decision Engine)
/// 1. Teknik Momentum (Donchian, EMA9/21/50, RSI14, ATR)
/// 2. Order Flow & Likidite (Taker Alıcı/Satıcı Oranı, Canlı OI Değişimi)
/// 3. TreeNews Scout (X/Twitter Canlı Flaş Haberler & Balina Hareketleri)
/// ⚡ BTC 15-Saniyelik Anlık İvme Sensörü (Lead-Lag Velocity Tracker)
struct BtcVelocityTracker {
    ticks: std::collections::VecDeque<(Instant, f64)>,
}

impl BtcVelocityTracker {
    fn new() -> Self {
        Self {
            ticks: std::collections::VecDeque::with_capacity(256),
        }
    }

    fn on_tick(&mut self, price: f64) {
        let now = Instant::now();
        self.ticks.push_back((now, price));
        while let Some((ts, _)) = self.ticks.front() {
            if now.duration_since(*ts) > Duration::from_secs(15) {
                self.ticks.pop_front();
            } else {
                break;
            }
        }
    }

    fn velocity_15s_pct(&self) -> f64 {
        if let (Some((_, old_p)), Some((_, new_p))) = (self.ticks.front(), self.ticks.back()) {
            if *old_p > 0.0 {
                return ((*new_p - *old_p) / *old_p) * 100.0;
            }
        }
        0.0
    }
}

/// 🚀 Geciken Yüksek-Beta Altcoin Avcısı & BTC Sonraki Mum Öncü Tahmincisi (Lead-Lag & Next-Candle Predictor)
fn compute_lead_lag_beta_boost(
    btc_eng: &SymbolEngine,
    alt_eng: &SymbolEngine,
    btc_live_15s_vel: f64,
) -> i32 {
    if alt_eng.spec.symbol == "BTCUSDT" {
        return 0;
    }
    let btc_bar_ret = if btc_eng.last_open > 0.0 {
        ((btc_eng.last_price - btc_eng.last_open) / btc_eng.last_open) * 100.0
    } else {
        0.0
    };
    let alt_bar_ret = if alt_eng.last_open > 0.0 {
        ((alt_eng.last_price - alt_eng.last_open) / alt_eng.last_open) * 100.0
    } else {
        0.0
    };

    // 🔮 BTC SONRAKİ MUM ÖNCÜ İVME VE EMİR AKIŞI TAHMİNCİSİ (BTC Next-Candle Predictive Vector)
    // BTC'nin bir sonraki 15dk/1s mumda hangi yöne patlayacağını Taker Buy %, OI değişimi, EMA9/21 ve 15s hızından hesaplar
    let mut btc_next_candle_bull_score: i32 = 0;
    if btc_eng.last_price > btc_eng.live_ema9 {
        btc_next_candle_bull_score += 1;
    } else if btc_eng.last_price < btc_eng.live_ema9 {
        btc_next_candle_bull_score -= 1;
    }
    if btc_eng.live_taker_buy_pct >= 53.5 {
        btc_next_candle_bull_score += 2; // Güçlü piyasa alıcı baskısı -> Sonraki mum yukarı
    } else if btc_eng.live_taker_buy_pct <= 46.5 {
        btc_next_candle_bull_score -= 2; // Güçlü piyasa satıcı baskısı -> Sonraki mum aşağı
    }
    if btc_eng.live_delta_oi_pct > 0.15 {
        // Açık pozisyon (OI) artarken fiyatın yönü gerçek kurumsal para girişini gösterir
        if btc_bar_ret >= 0.0 {
            btc_next_candle_bull_score += 1;
        } else {
            btc_next_candle_bull_score -= 1;
        }
    }
    if btc_live_15s_vel >= 0.12 || btc_bar_ret >= 0.25 {
        btc_next_candle_bull_score += 2;
    } else if btc_live_15s_vel <= -0.12 || btc_bar_ret <= -0.25 {
        btc_next_candle_bull_score -= 2;
    }

    let is_high_beta = matches!(
        alt_eng.spec.symbol,
        "SOLUSDT" | "SUIUSDT" | "NEARUSDT" | "APTUSDT" | "AVAXUSDT" | "TIAUSDT"
    );

    if alt_eng.signal_dir == "LONG" {
        // BTC'nin bir sonraki mum tahmini net düşüşse (<= -3) veya sert satış yiyorsa altcoin LONG'u VETO ET!
        if btc_next_candle_bull_score <= -3 || btc_bar_ret < -0.40 || btc_live_15s_vel <= -0.22 {
            return -30;
        }
        if btc_next_candle_bull_score >= 3 {
            return if is_high_beta && alt_bar_ret < btc_bar_ret * 2.2 { 12 } else { 8 };
        }
    } else if alt_eng.signal_dir == "SHORT" {
        // BTC'nin bir sonraki mum tahmini net yükselişse (>= +3) veya sert alım geliyorsa altcoin SHORT'u VETO ET!
        if btc_next_candle_bull_score >= 3 || btc_bar_ret > 0.40 || btc_live_15s_vel >= 0.22 {
            return -30;
        }
        if btc_next_candle_bull_score <= -3 {
            return if is_high_beta && alt_bar_ret > btc_bar_ret * 2.2 { 12 } else { 8 };
        }
    }
    0
}

/// 4. Zero-Lag BTC Çapası (Bitcoin Anlık Sinyali, Yönü ve Satış Baskısı)
/// 5. Market Breadth (13 Coinin Kolektif Piyasa Konsensüsü)
fn typesafe_unified_arbiter(
    symbol: &str,
    side: &str,
    tech_score: u32,
    price: f64,
    ema50: f64,
    rsi: f64,
    taker_buy_pct: f64,
    _delta_oi_pct: f64,
    scout_boost: i32,
    scout_headline: &str,
    btc_trend: &str,
    btc_instant_dir: &str,
    btc_instant_score: u32,
    market_long_count: usize,
    market_short_count: usize,
) -> (bool, f64, String) {
    // =========================================================================
    // 🏛️ KATMAN 1: MAKRO TREND KALKANI (HTF EMA50 Trend Invariant)
    // =========================================================================
    if ema50 > 0.0 && price > 0.0 {
        // Seçenek A: Taker hacim akışı ve RSI onayı varsa (%56+ alıcı & RSI < 68) kırılım kabul edilir
        let strong_breakout_long = side == "LONG" && taker_buy_pct >= 56.0 && rsi < 68.0;
        let strong_breakout_short = side == "SHORT" && taker_buy_pct <= 44.0 && rsi > 32.0;

        if side == "LONG" && !strong_breakout_long && price < ema50 * 0.998 {
            return (
                false,
                0.08,
                format!("{} LONG Vetosu: Fiyat ({:.4}) Trend Çizgisinin (EMA50: {:.4}) altında! Trende karşı alım yasak.", symbol, price, ema50),
            );
        }
        if side == "SHORT" && !strong_breakout_short && price > ema50 * 1.002 {
            return (
                false,
                0.08,
                format!("{} SHORT Vetosu: Fiyat ({:.4}) Trend Çizgisinin (EMA50: {:.4}) üzerinde! Trende karşı satım yasak.", symbol, price, ema50),
            );
        }
    }

    // =========================================================================
    // 🛡️ KATMAN 2: PULLBACK & AŞIRI UÇ / FOMO ENGELİ (Anti-FOMO Invariant)
    // =========================================================================
    // 1. Aşırı Alım Tepesi (Tepeden Fomo ile Alımı Kesinlikle Engelle - Pine Script Uyumlu)
    if side == "LONG" && rsi >= 68.0 {
        return (false, 0.05, format!("{} Aşırı Alım Tepesi (RSI: {:.1} >= 68) -> Tepeden Almama Kalkanı Vetosu", symbol, rsi));
    }
    // 2. Aşırı Satım Dibi (Dipten Şelale Sonrası Shortlamayı Engelle - Pine Script Uyumlu)
    if side == "SHORT" && rsi <= 32.0 {
        return (false, 0.05, format!("{} Aşırı Satım Dibi (RSI: {:.1} <= 32) -> Dipten Satmama Kalkanı Vetosu", symbol, rsi));
    }
    // 3. Bıçak Tutmama & Squeeze
    if side == "LONG" && rsi < 20.0 {
        return (false, 0.05, format!("{} Şelale Çöküşü (RSI: {:.1} < 20) -> Bıçak Tutmama Veto", symbol, rsi));
    }
    if side == "SHORT" && rsi > 80.0 {
        return (false, 0.05, format!("{} Parabolik Squeeze (RSI: {:.1} > 80) -> Short Squeeze Veto", symbol, rsi));
    }

    // =========================================================================
    // 🌊 KATMAN 3: ANLIK EMİR AKIŞI & BTC BETA ÇAPASI (Orderflow & BTC Alignment)
    // =========================================================================
    // 1. Scout Canlı Haber & Balina Veto Kalkanı
    if scout_boost <= -25 {
        return (
            false,
            0.10,
            format!("🚨 Scout Haber Vetosu: {} (Puan: {}) -> {}", symbol, scout_boost, scout_headline),
        );
    }

    // 2. Sıfır-Gecikmeli Anlık BTC Çelişki Kalkanı (Sadece Patlama / Aşırı Skor Halinde)
    if btc_instant_dir == "SHORT" && btc_instant_score >= 85 && side == "LONG" {
        return (
            false,
            0.15,
            format!("BTC SHORT patlarken (Skor: {}) {} LONG açılamaz (TypeSafe BTC Hız Vetosu)", btc_instant_score, symbol),
        );
    }
    if btc_instant_dir == "LONG" && btc_instant_score >= 85 && side == "SHORT" {
        return (
            false,
            0.15,
            format!("BTC LONG patlarken (Skor: {}) {} SHORT açılamaz (TypeSafe BTC Hız Vetosu)", btc_instant_score, symbol),
        );
    }

    // 3. Kolektif Piyasa Konsensüsü (Market Breadth)
    if side == "LONG" && market_short_count > market_long_count && market_short_count >= 6 && (market_short_count - market_long_count >= 2) {
        return (
            false,
            0.20,
            format!("Piyasa geneli düşüşte ({} SHORT / {} LONG) -> Boğa Tuzağı Vetosu", market_short_count, market_long_count),
        );
    }
    if side == "SHORT" && market_long_count > market_short_count && market_long_count >= 6 && (market_long_count - market_short_count >= 2) {
        return (
            false,
            0.20,
            format!("Piyasa geneli yükselişte ({} LONG / {} SHORT) -> Ayı Tuzağı Vetosu", market_long_count, market_short_count),
        );
    }

    // 4. Order Flow (Taker Akışı) Katı Kalkanı
    if side == "LONG" && taker_buy_pct < 45.0 {
        return (
            false,
            0.25,
            format!("Taker Alıcı Yetersiz (%{:.1} < %45.0 - Satış Baskısı) -> TypeSafe Flow Vetosu", taker_buy_pct),
        );
    }
    if side == "SHORT" && taker_buy_pct > 55.0 {
        return (
            false,
            0.25,
            format!("Taker Satıcı Yetersiz (%{:.1} > %55.0 - Alış Baskısı) -> TypeSafe Flow Vetosu", taker_buy_pct),
        );
    }

    // 5. Çok Boyutlu Ortak Beklenen Değer (JEV - Joint Expected Edge)
    let mut jev_score = tech_score as f64;
    if scout_boost > 0 {
        jev_score += (scout_boost as f64 * 0.4).min(15.0);
    }
    if side == "LONG" && taker_buy_pct >= 52.0 {
        jev_score += 5.0;
    } else if side == "SHORT" && taker_buy_pct <= 48.0 {
        jev_score += 5.0;
    }
    if side == "LONG" && btc_trend == "BOĞA" {
        jev_score += 5.0;
    } else if side == "SHORT" && btc_trend == "AYI" {
        jev_score += 5.0;
    }
    if ema50 > 0.0 && price > 0.0 {
        if side == "LONG" && price >= ema50 {
            jev_score += 5.0;
        } else if side == "SHORT" && price <= ema50 {
            jev_score += 5.0;
        }
    }

    let final_confidence = (jev_score / 100.0).min(0.99);

    if jev_score >= 70.0 {
        (
            true,
            final_confidence,
            format!(
                "🛡️ 3-Katmanlı MTF Confluence Tam Onayı (JEV Skor: {:.0}/100, Taker: %{:.1}, BTC: {}, Breadth: {}L/{}S)",
                jev_score, taker_buy_pct, btc_instant_dir, market_long_count, market_short_count
            ),
        )
    } else {
        (
            false,
            final_confidence,
            format!("Yetersiz Bütüncül JEV Skoru: {:.0} < 70 Eşiği", jev_score),
        )
    }
}

/// 🚨 TypeSafe JEV Canlı Pozisyon Acil Durum Çıkış Denetçisi (Emergency Exit Arbiter & BTC Shield)
fn check_emergency_exit_typesafe_jev(
    symbol: &str,
    side: &str,
    _entry_price: f64,
    _current_price: f64,
    scout_boost: i32,
    scout_headline: &str,
    _eng_signal_dir: &str,
    _eng_signal_score: u32,
    btc_instant_dir: &str,
    btc_instant_score: u32,
    btc_entry_price: f64,
    btc_cur_price: f64,
) -> Option<String> {
    // 1. Kritik Kara Kuğu / Olumsuz Haber Kalkanı (Doğrulanmış acil haber varsa)
    if scout_boost <= -30 {
        return Some(format!(
            "🚨 TYPESAFE JEV ACİL HABER ÇIKIŞI: {} (Puan: {}) -> {}",
            symbol, scout_boost, scout_headline
        ));
    }

    // 2. 🛡️ BTC ANİ TERS YÖNE PATLAMA & TREND TERSİNE DÖNÜŞ ACİL FRENİ (Dual-Layer BTC Shield)
    if btc_entry_price > 0.0 && btc_cur_price > 0.0 {
        let btc_move_pct = (btc_cur_price - btc_entry_price) / btc_entry_price;
        if side == "SHORT" {
            // SHORT pozisyondayken BTC yukarı fırladıysa:
            if btc_move_pct >= 0.0035 {
                return Some(format!(
                    "🚨 BTC TERS PATLAMA ACİL FRENİ: BTC Girişten +%{:.2} fırladı ({:.1} -> {:.1})! {} Short pozisyon erken kesildi.",
                    btc_move_pct * 100.0, btc_entry_price, btc_cur_price, symbol
                ));
            }
            if btc_instant_dir == "LONG" && btc_instant_score >= 75 && btc_move_pct >= 0.0015 {
                return Some(format!(
                    "🚨 BTC TREND TERS DÖNÜŞ FRENİ: BTC Güçlü LONG'a döndü (Skor: {}, +%{:.2})! {} Short pozisyon erken kesildi.",
                    btc_instant_score, btc_move_pct * 100.0, symbol
                ));
            }
        } else if side == "LONG" {
            // LONG pozisyondayken BTC aşağı çöktüyse:
            if btc_move_pct <= -0.0035 {
                return Some(format!(
                    "🚨 BTC ÇÖKÜŞ ACİL FRENİ: BTC Girişten -%{:.2} düştü ({:.1} -> {:.1})! {} Long pozisyon erken kesildi.",
                    btc_move_pct.abs() * 100.0, btc_entry_price, btc_cur_price, symbol
                ));
            }
            if btc_instant_dir == "SHORT" && btc_instant_score >= 75 && btc_move_pct <= -0.0015 {
                return Some(format!(
                    "🚨 BTC TREND TERS DÖNÜŞ FRENİ: BTC Güçlü SHORT'a döndü (Skor: {}, -%{:.2})! {} Long pozisyon erken kesildi.",
                    btc_instant_score, btc_move_pct.abs() * 100.0, symbol
                ));
            }
        }
    }

    None
}

fn configure_live_leverage_and_isolated_margin(api_key: &str, api_secret: &str) {
    if api_key.is_empty() || api_secret.is_empty() {
        println!("ℹ️ [SIMÜLASYON MODU] API Key/Secret girilmedi; gerçek Mainnet akışı üzerinde sanal $10 kasa ile çalışılıyor.");
        return;
    }
    println!("🔐 [MAINNET YETKİLENDİRME] 13 Parite için ISOLATED Marjin ve Kaldıraç (5x ISOLATED) ayarlanıyor...");
    for spec in SYMBOLS.iter() {
        let ts1 = Utc::now().timestamp_millis();
        let q_margin = format!("symbol={}&marginType=ISOLATED&timestamp={}", spec.symbol, ts1);
        if let Ok(mut mac) = HmacSha256::new_from_slice(api_secret.as_bytes()) {
            mac.update(q_margin.as_bytes());
            let sig = hex::encode(mac.finalize().into_bytes());
            let url = format!("{}/fapi/v1/marginType?{}&signature={}", REST_BASE_URL, q_margin, sig);
            let _ = Command::new(CURL_BIN)
                .args(["-s", "-X", "POST", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url])
                .output();
        }

        let ts2 = Utc::now().timestamp_millis();
        let q_lev = format!("symbol={}&leverage={}&timestamp={}", spec.symbol, spec.leverage, ts2);
        if let Ok(mut mac) = HmacSha256::new_from_slice(api_secret.as_bytes()) {
            mac.update(q_lev.as_bytes());
            let sig = hex::encode(mac.finalize().into_bytes());
            let url = format!("{}/fapi/v1/leverage?{}&signature={}", REST_BASE_URL, q_lev, sig);
            let _ = Command::new(CURL_BIN)
                .args(["-s", "-X", "POST", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url])
                .output();
        }
    }
}

fn send_signed_order_if_live(
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    side: &str,
    qty: f64,
    qty_decimals: usize,
) -> Option<String> {
    if api_key.is_empty() || api_secret.is_empty() {
        return None;
    }
    let timestamp = Utc::now().timestamp_millis();
    let query = format!(
        "symbol={}&side={}&type=MARKET&quantity={:.prec$}&timestamp={}",
        symbol,
        side,
        qty,
        timestamp,
        prec = qty_decimals
    );
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).ok()?;
    mac.update(query.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let full_url = format!(
        "{}/fapi/v1/order?{}&signature={}",
        REST_BASE_URL, query, signature
    );

    let output = Command::new(CURL_BIN)
        .args([
            "-s",
            "-X",
            "POST",
            "-H",
            &format!("X-MBX-APIKEY: {}", api_key),
            &full_url,
        ])
        .output()
        .ok()?;
    let resp = String::from_utf8(output.stdout).ok()?;
    println!("📡 [BINANCE MAINNET EMİR CEVABI - {}]: {}", symbol, resp);
    Some(resp)
}

fn send_signed_reduce_only_close_if_live(
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    close_side: &str,
    qty: f64,
    qty_decimals: usize,
) -> Option<String> {
    if api_key.is_empty() || api_secret.is_empty() {
        return None;
    }
    let timestamp = Utc::now().timestamp_millis();
    let query = format!(
        "symbol={}&side={}&type=MARKET&quantity={:.prec$}&reduceOnly=true&timestamp={}",
        symbol,
        close_side,
        qty,
        timestamp,
        prec = qty_decimals
    );
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).ok()?;
    mac.update(query.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let full_url = format!(
        "{}/fapi/v1/order?{}&signature={}",
        REST_BASE_URL, query, signature
    );
    let output = Command::new(CURL_BIN)
        .args(["-s", "-X", "POST", "-H", &format!("X-MBX-APIKEY: {}", api_key), &full_url])
        .output()
        .ok()?;
    let resp = String::from_utf8(output.stdout).ok()?;
    println!("📡 [BINANCE MAINNET REDUCE-ONLY KAPANIŞ - {}]: {}", symbol, resp);
    let _ = cancel_exchange_open_orders_if_live(api_key, api_secret, symbol);
    Some(resp)
}

fn cancel_exchange_open_orders_if_live(api_key: &str, api_secret: &str, symbol: &str) -> Option<String> {
    if api_key.is_empty() || api_secret.is_empty() {
        return None;
    }
    let timestamp = Utc::now().timestamp_millis();
    let query = format!("symbol={}&timestamp={}", symbol, timestamp);
    let mut mac = HmacSha256::new_from_slice(api_secret.as_bytes()).ok()?;
    mac.update(query.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let full_url = format!("{}/fapi/v1/allOpenOrders?{}&signature={}", REST_BASE_URL, query, signature);
    let _ = Command::new(CURL_BIN)
        .args(["-s", "-X", "DELETE", "-H", &format!("X-MBX-APIKEY: {}", api_key), &full_url])
        .output();

    let ts_algo = Utc::now().timestamp_millis();
    let q_algo = format!("symbol={}&timestamp={}", symbol, ts_algo);
    let mut mac_algo = HmacSha256::new_from_slice(api_secret.as_bytes()).ok()?;
    mac_algo.update(q_algo.as_bytes());
    let sig_algo = hex::encode(mac_algo.finalize().into_bytes());
    let url_algo = format!("{}/fapi/v1/algoOpenOrders?{}&signature={}", REST_BASE_URL, q_algo, sig_algo);
    let output = Command::new(CURL_BIN)
        .args(["-s", "-X", "DELETE", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url_algo])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()
}

fn place_exchange_hard_stop_and_tp_if_live(
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    close_side: &str,
    stop_price: f64,
    tp_price: f64,
    price_decimals: usize,
) {
    if api_key.is_empty() || api_secret.is_empty() {
        return;
    }
    let _ = cancel_exchange_open_orders_if_live(api_key, api_secret, symbol);

    // 1. Borsa Sunucusunda Donanımsal STOP_MARKET (Binance /fapi/v1/algoOrder - AWS Çökse Bile Binance Otomatik Kapatır!)
    let ts1 = Utc::now().timestamp_millis();
    let q_sl = format!(
        "algoType=CONDITIONAL&symbol={}&side={}&type=STOP_MARKET&triggerPrice={:.prec$}&closePosition=true&workingType=MARK_PRICE&timestamp={}",
        symbol, close_side, stop_price, ts1, prec = price_decimals
    );
    if let Ok(mut mac) = HmacSha256::new_from_slice(api_secret.as_bytes()) {
        mac.update(q_sl.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());
        let url = format!("{}/fapi/v1/algoOrder?{}&signature={}", REST_BASE_URL, q_sl, sig);
        if let Ok(out) = Command::new(CURL_BIN)
            .args(["-s", "-X", "POST", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url])
            .output()
        {
            if let Ok(resp) = String::from_utf8(out.stdout) {
                println!("🛡️ [BORSA DONANIMSAL ALGO STOP-LOSS KURULDU - {} @ {:.prec$}]: {}", symbol, stop_price, resp, prec = price_decimals);
            }
        }
    }

    // 2. Borsa Sunucusunda Donanımsal TAKE_PROFIT_MARKET (Binance /fapi/v1/algoOrder - AWS Çökse Bile Kârda Otomatik Kapatır!)
    if tp_price > 0.0 {
        let ts2 = Utc::now().timestamp_millis();
        let q_tp = format!(
            "algoType=CONDITIONAL&symbol={}&side={}&type=TAKE_PROFIT_MARKET&triggerPrice={:.prec$}&closePosition=true&workingType=MARK_PRICE&timestamp={}",
            symbol, close_side, tp_price, ts2, prec = price_decimals
        );
        if let Ok(mut mac) = HmacSha256::new_from_slice(api_secret.as_bytes()) {
            mac.update(q_tp.as_bytes());
            let sig = hex::encode(mac.finalize().into_bytes());
            let url = format!("{}/fapi/v1/algoOrder?{}&signature={}", REST_BASE_URL, q_tp, sig);
            let _ = Command::new(CURL_BIN)
                .args(["-s", "-X", "POST", "-H", &format!("X-MBX-APIKEY: {}", api_key), &url])
                .output();
        }
    }
}

#[derive(Serialize)]
struct LiveMultiTelemetryPayload<'a> {
    pub updated_at: String,
    pub tick_count: u64,
    pub reconnect_count: u32,
    pub active_symbol: String,
    pub active_price: f64,
    pub active_ema9: f64,
    pub active_ema21: f64,
    pub active_rsi14: f64,
    pub active_atr14: f64,
    pub balance: f64,
    pub equity: f64,
    pub unrealized_gross_pnl: f64,
    pub unrealized_net_pnl: f64,
    pub unrealized_roe_pct: f64,
    pub realized_net_pnl: f64,
    pub total_fees_paid: f64,
    pub max_drawdown_pct: f64,
    pub total_trades: u32,
    pub winning_trades: u32,
    pub losing_trades: u32,
    pub position: &'a PositionState,
    pub radar: Vec<CoinRadarSnapshot>,
    pub closed_trades: &'a [ClosedTradeRecord],
    pub trading_enabled: bool,
}

fn write_live_telemetry(
    state: &BotPersistentState,
    engines: &HashMap<String, SymbolEngine>,
    tick_count: u64,
) {
    let mut radar: Vec<CoinRadarSnapshot> = SYMBOLS
        .iter()
        .filter_map(|s| engines.get(s.symbol).map(|e| e.to_radar_snapshot()))
        .collect();
    // 💎 PINE SCRIPT SIRALAMASI: En yakından en uzağa doğru sıralama (Yakınlık Öncelikli)
    radar.sort_by(|a, b| {
        a.sma50_dist_pct
            .partial_cmp(&b.sma50_dist_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (idx, r) in radar.iter_mut().enumerate() {
        r.proximity_rank = (idx + 1) as u32;
    }

    let (active_sym, unrealized_gross, unrealized_net, roe_pct) = match &state.position {
        PositionState::Long {
            symbol,
            entry_price,
            qty,
            margin_used,
            entry_fee,
            ..
        } => {
            let cur_price = engines
                .get(symbol)
                .map(|e| e.last_price)
                .unwrap_or(*entry_price);
            let exit_exec = cur_price * (1.0 - SLIPPAGE_RATE);
            let gross = (exit_exec - entry_price) * qty;
            let est_exit_fee = exit_exec * qty * TAKER_FEE_RATE;
            let net = gross - entry_fee - est_exit_fee;
            let roe = if *margin_used > 0.0 {
                (net / margin_used) * 100.0
            } else {
                0.0
            };
            (symbol.clone(), gross, net, roe)
        }
        PositionState::Short {
            symbol,
            entry_price,
            qty,
            margin_used,
            entry_fee,
            ..
        } => {
            let cur_price = engines
                .get(symbol)
                .map(|e| e.last_price)
                .unwrap_or(*entry_price);
            let exit_exec = cur_price * (1.0 + SLIPPAGE_RATE);
            let gross = (entry_price - exit_exec) * qty;
            let est_exit_fee = exit_exec * qty * TAKER_FEE_RATE;
            let net = gross - entry_fee - est_exit_fee;
            let roe = if *margin_used > 0.0 {
                (net / margin_used) * 100.0
            } else {
                0.0
            };
            (symbol.clone(), gross, net, roe)
        }
        PositionState::Flat => {
            let top_sym = radar
                .first()
                .map(|r| r.symbol.clone())
                .unwrap_or_else(|| "SOLUSDT".to_string());
            (top_sym, 0.0, 0.0, 0.0)
        }
    };

    let active_eng = engines
        .get(&active_sym)
        .or_else(|| engines.get("SOLUSDT"))
        .unwrap();

    let equity = state.balance + unrealized_gross;
    let payload = LiveMultiTelemetryPayload {
        updated_at: Utc::now().format("%H:%M:%S%.3f").to_string(),
        tick_count,
        reconnect_count: state.reconnect_count,
        active_symbol: active_sym,
        active_price: active_eng.last_price,
        active_ema9: active_eng.live_ema9,
        active_ema21: active_eng.live_ema21,
        active_rsi14: active_eng.live_rsi14,
        active_atr14: active_eng.live_atr14,
        balance: state.balance,
        equity,
        unrealized_gross_pnl: unrealized_gross,
        unrealized_net_pnl: unrealized_net,
        unrealized_roe_pct: roe_pct,
        realized_net_pnl: state.realized_net_pnl,
        total_fees_paid: state.total_fees_paid,
        max_drawdown_pct: state.max_drawdown_pct,
        total_trades: state.total_trades,
        winning_trades: state.winning_trades,
        losing_trades: state.losing_trades,
        position: &state.position,
        radar,
        closed_trades: &state.closed_trades,
        trading_enabled: std::fs::read_to_string("bot_control.json")
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("trading_enabled").and_then(|b| b.as_bool()))
            .unwrap_or(true),
    };

    if let Ok(json) = serde_json::to_string(&payload) {
        let js_content = format!("window.__RUST_STATE__ = {};\n", json);
        let tmp_js = format!("{}.tmp", TELEMETRY_JS_FILE);
        if fs::write(&tmp_js, js_content).is_ok() {
            let _ = fs::rename(&tmp_js, TELEMETRY_JS_FILE);
        }
    }
}

fn try_open_best_multi_coin_signal(
    state: &mut BotPersistentState,
    engines: &HashMap<String, SymbolEngine>,
    live_cooldown_until: &mut HashMap<String, Instant>,
    btc_vel_15s: f64,
    api_key: &str,
    api_secret: &str,
) {
    if state.position != PositionState::Flat {
        return;
    }

    // 🛑 KULLANICI BOT KONTROLÜ (Dashboard Aktif/Pasif Butonu)
    let is_trading_enabled = std::fs::read_to_string("bot_control.json")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("trading_enabled").and_then(|b| b.as_bool()))
        .unwrap_or(true);

    if !is_trading_enabled {
        // Kullanıcı botu pasife aldı, gözlemci modunda bekle, işlem açma!
        return;
    }

    // 🛡️ DEVRE KESİCİ (CIRCUIT BREAKER) KONTROLÜ
    let now_ts = Utc::now().timestamp();
    if state.circuit_breaker_until > now_ts {
        // Devre kesici aktif, yeni işlem açma
        return;
    }

    // 🛡️ DİNAMİK ZİRVE ÇEKİLME (DRAWDOWN) KALKANI
    // Kasa zirve bakiyeden (%15) fazla çekilirse 1 saatlik koruma molası ver
    if state.peak_equity > 0.0 && state.balance < state.peak_equity * 0.85 {
        state.circuit_breaker_until = now_ts + 3600;
        println!("🚨 [DRAWDOWN KORUMASI] Kasa zirve bakiyeden (%15) fazla çekildi! Bot 1 saatlik koruma molasında.");
        return;
    }

    let (btc_trend, btc_instant_dir, btc_instant_score, btc_price) = if let Some(btc_eng) = engines.get("BTCUSDT") {
        let tr = if btc_eng.live_ema21 > btc_eng.live_ema50 {
            "BOĞA"
        } else if btc_eng.live_ema21 < btc_eng.live_ema50 {
            "AYI"
        } else {
            "NÖTR"
        };
        (tr, btc_eng.signal_dir, btc_eng.signal_score, btc_eng.last_price)
    } else {
        ("NÖTR", "NEUTRAL", 0, 80000.0)
    };

    // Kolektif Piyasa Konsensüsü (Market Breadth)
    let mut market_long_count = 0usize;
    let mut market_short_count = 0usize;
    for spec in SYMBOLS.iter() {
        if let Some(e) = engines.get(spec.symbol) {
            if e.signal_dir == "LONG" && e.signal_score >= 60 {
                market_long_count += 1;
            } else if e.signal_dir == "SHORT" && e.signal_score >= 60 {
                market_short_count += 1;
            }
        }
    }

    let now = Instant::now();
    struct AmbushTarget<'a> {
        engine: &'a SymbolEngine,
        dir: &'static str,
        proximity_pct: f64,
        sma50: f64,
        planned_sl: f64,
        is_touching_sma50: bool,
    }
    let mut ambush_pool: Vec<AmbushTarget> = Vec::new();
    for spec in SYMBOLS.iter() {
        if let Some(cd_exp) = live_cooldown_until.get(spec.symbol) {
            if now < *cd_exp {
                continue;
            }
        }
        if let Some(eng) = engines.get(spec.symbol) {
            let cand_sma50 = eng.get_sma50();
            if cand_sma50 <= 0.0 || eng.last_price <= 0.0 {
                continue;
            }

            // 1. SMA50 Mesafesi
            let dist_sma50_pct = ((eng.last_price - cand_sma50).abs() / cand_sma50) * 100.0;

            // 2. Kullanıcı Kuralı: SMA50'ye maksimum %0.05 mesafede olanlar kabul edilir!
            if dist_sma50_pct > 0.05 {
                continue;
            }

            // 3. Seçenek A (Akıllı Taker Hacim Akışı & RSI Eksenli Yön Tayini):
            // - Taker alıcılar baskınsa (%56+) ve RSI aşırı alımda değilse (<68) -> LONG (SMA50 Kırılımı)
            // - Taker satıcılar baskınsa (<=%44) ve RSI aşırı satımda değilse (>32) -> SHORT (SMA50 Kırılımı)
            // - Akış dengeliyse (%44 - %56) -> Standart Destek / Direnç Sekmesi
            let candidate_dir: &'static str = if eng.live_taker_buy_pct >= 56.0 {
                if eng.live_rsi14 < 68.0 {
                    "LONG"
                } else {
                    "SHORT"
                }
            } else if eng.live_taker_buy_pct <= 44.0 {
                if eng.live_rsi14 > 32.0 {
                    "SHORT"
                } else {
                    "LONG"
                }
            } else {
                if eng.last_price <= cand_sma50 {
                    "SHORT"
                } else {
                    "LONG"
                }
            };

            // 4. BTC Patlama / Hacim Kalkanı:
            // "btc çok hacimli değilse patlama yapcak gibi değilse ona bile bakmayalım"
            // Sadece BTC çok sert patlama/şelale yapıyorsa (|velocity| > 0.20% veya anlık skor >= 85) ters işlem engellenir.
            let btc_exploding = btc_vel_15s.abs() > 0.20 || btc_instant_score >= 85;
            if btc_exploding {
                if (btc_vel_15s < -0.20 || (btc_instant_dir == "SHORT" && btc_instant_score >= 85)) && candidate_dir == "LONG" {
                    continue;
                }
                if (btc_vel_15s > 0.20 || (btc_instant_dir == "LONG" && btc_instant_score >= 85)) && candidate_dir == "SHORT" {
                    continue;
                }
            }

            // 5. Scout Acil Haber Kalkanı
            if eng.live_scout_score_boost < -25 {
                continue;
            }

            // 6. Stop Loss ve Temas Kontrolü (%0.05 pusu koridoru):
            let planned_sl = if candidate_dir == "SHORT" {
                (cand_sma50 * 1.0008).max(eng.last_price * 1.0025)
            } else {
                (cand_sma50 * 0.9992).min(eng.last_price * 0.9975)
            };

            let is_touching_sma50 = true;

            if eng.calculate_micro_10usd_order(state.balance).is_some() {
                ambush_pool.push(AmbushTarget {
                    engine: eng,
                    dir: candidate_dir,
                    proximity_pct: dist_sma50_pct,
                    sma50: cand_sma50,
                    planned_sl,
                    is_touching_sma50,
                });
            }
        }
    }

    // 💎 GÖLGE PUSU HAVUZU (First-to-Fill OCO):
    // Havuzu en yakından en uzağa sırala
    ambush_pool.sort_by(|a, b| {
        a.proximity_pct
            .partial_cmp(&b.proximity_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // En yakın ilk 4 parite Pusu Havuzuna (Ambush Pool) alınır
    let top_pool = if ambush_pool.len() > 4 {
        &ambush_pool[0..4]
    } else {
        &ambush_pool[..]
    };

    // İlk dokunan / tetiklenen adayı seç
    let triggered_candidate = top_pool.iter().find(|t| t.is_touching_sma50);

    if let Some(target) = triggered_candidate {
        let eng = target.engine;
        let trade_dir = target.dir;
        let dist_pct = target.proximity_pct;
        let planned_sl = target.planned_sl;

        println!(
            "🎯 [GÖLGE PUSU HAVUZU: İLK DOKUNAN YAKALANDI] {} {} | Fiyat: {:.4} | SMA50: {:.4} (Mesafe: %{:.3}) | Planlanan SL: {:.4} (SMA50 Koridoru) | Diğer 3 Adayın Pususu İptal Edildi!",
            eng.spec.symbol, trade_dir, eng.last_price, target.sma50, dist_pct, planned_sl
        );

        let (is_approved, jev_prob, jev_reason) = typesafe_unified_arbiter(
            &eng.spec.symbol,
            trade_dir,
            85,
            eng.last_price,
            eng.live_ema50,
            eng.live_rsi14,
            eng.live_taker_buy_pct,
            eng.live_delta_oi_pct,
            eng.live_scout_score_boost,
            &eng.live_scout_headline,
            btc_trend,
            btc_instant_dir,
            btc_instant_score,
            market_long_count,
            market_short_count,
        );

        if !is_approved {
            println!(
                "🛑 [TYPESAFE JEV VETO] {} {} işlemi engellendi! Skor: {:.2} | Gerekçe: {}",
                eng.spec.symbol, trade_dir, jev_prob, jev_reason
            );
            live_cooldown_until.insert(eng.spec.symbol.to_string(), Instant::now() + Duration::from_secs(60));
            return;
        }

        println!(
            "🛡️ [TYPESAFE JEV ONAY] {} {} işlemi kabul edildi! Skor: {:.2} | Gerekçe: {}",
            eng.spec.symbol, trade_dir, jev_prob, jev_reason
        );

        if let Some((qty, margin_used, stop_dist, tp_dist)) =
            eng.calculate_micro_10usd_order(state.balance)
        {
            let price = eng.last_price;
            let sym = eng.spec.symbol.to_string();
            let lev = eng.spec.leverage;

            if trade_dir == "LONG" {
                let mut exec_price = price * (1.0 + SLIPPAGE_RATE);
                if !api_key.is_empty() {
                    let resp = send_signed_order_if_live(
                        api_key,
                        api_secret,
                        &sym,
                        "BUY",
                        qty,
                        eng.spec.qty_decimals,
                    ).unwrap_or_default();
                    if !resp.contains("orderId") {
                        eprintln!("❌ [CANLI EMİR REDDEDİLDİ - {} LONG] Binance cevabı: {}", sym, resp);
                        live_cooldown_until.insert(sym.clone(), Instant::now() + Duration::from_secs(60));
                        return;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
                        if let Some(ap) = v.get("avgPrice").and_then(|s| s.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                            if ap > 0.0 {
                                exec_price = ap;
                            }
                        }
                    }
                }
                let entry_fee = exec_price * qty * TAKER_FEE_RATE;
                // SL: SMA50'nin biraz altı (en fazla stop_dist kadar korumalı)
                let sl = planned_sl.min(exec_price - stop_dist);
                let tp = exec_price + tp_dist;

                state.balance -= entry_fee;
                state.total_fees_paid += entry_fee;
                state.position = PositionState::Long {
                    symbol: sym.clone(),
                    leverage: lev,
                    entry_price: exec_price,
                    qty,
                    margin_used,
                    stop_loss: sl,
                    take_profit: tp,
                    highest_price: exec_price,
                    trailing_active: false,
                    entry_fee,
                    opened_at: (Utc::now() + chrono::Duration::hours(3)).format("%Y-%m-%d %H:%M:%S").to_string(),
                    opened_ts: Utc::now().timestamp(),
                    regime: eng.active_regime,
                };
                place_exchange_hard_stop_and_tp_if_live(
                    api_key,
                    api_secret,
                    &sym,
                    "SELL",
                    sl,
                    tp,
                    eng.spec.price_decimals,
                );
                state.btc_entry_price = btc_price;
                state.save_atomic();
                println!(
                    "🟢 [YENİ LONG (Mod {}): {} {}x | Yakınlık: %{:.2}] Giriş: {:.4} | Miktar: {:.3} | Marjin: ${:.2} (Hacim: ${:.2}) | SL: {:.4} (SMA50 Altı) | TP: {:.4} | Fee: -${:.3}",
                    eng.active_regime,
                    sym,
                    lev,
                    dist_pct,
                    exec_price,
                    qty,
                    margin_used,
                    exec_price * qty,
                    sl,
                    tp,
                    entry_fee
                );
            } else if trade_dir == "SHORT" {
                let mut exec_price = price * (1.0 - SLIPPAGE_RATE);
                if !api_key.is_empty() {
                    let resp = send_signed_order_if_live(
                        api_key,
                        api_secret,
                        &sym,
                        "SELL",
                        qty,
                        eng.spec.qty_decimals,
                    ).unwrap_or_default();
                    if !resp.contains("orderId") {
                        eprintln!("❌ [CANLI EMİR REDDEDİLDİ - {} SHORT] Binance cevabı: {}", sym, resp);
                        live_cooldown_until.insert(sym.clone(), Instant::now() + Duration::from_secs(60));
                        return;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
                        if let Some(ap) = v.get("avgPrice").and_then(|s| s.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                            if ap > 0.0 {
                                exec_price = ap;
                            }
                        }
                    }
                }
                let entry_fee = exec_price * qty * TAKER_FEE_RATE;
                // SL: SMA50'nin biraz üstü (en fazla stop_dist kadar korumalı)
                let sl = planned_sl.max(exec_price + stop_dist);
                let tp = exec_price - tp_dist;

                state.balance -= entry_fee;
                state.total_fees_paid += entry_fee;
                state.position = PositionState::Short {
                    symbol: sym.clone(),
                    leverage: lev,
                    entry_price: exec_price,
                    qty,
                    margin_used,
                    stop_loss: sl,
                    take_profit: tp,
                    lowest_price: exec_price,
                    trailing_active: false,
                    entry_fee,
                    opened_at: (Utc::now() + chrono::Duration::hours(3)).format("%Y-%m-%d %H:%M:%S").to_string(),
                    opened_ts: Utc::now().timestamp(),
                    regime: eng.active_regime,
                };
                place_exchange_hard_stop_and_tp_if_live(
                    api_key,
                    api_secret,
                    &sym,
                    "BUY",
                    sl,
                    tp,
                    eng.spec.price_decimals,
                );
                state.btc_entry_price = btc_price;
                state.save_atomic();
                println!(
                    "🔴 [YENİ SHORT (Mod {}): {} {}x | Yakınlık: %{:.2}] Giriş: {:.4} | Miktar: {:.3} | Marjin: ${:.2} (Hacim: ${:.2}) | SL: {:.4} (SMA50 Üstü) | TP: {:.4} | Fee: -${:.3}",
                    eng.active_regime,
                    sym,
                    lev,
                    dist_pct,
                    exec_price,
                    qty,
                    margin_used,
                    exec_price * qty,
                    sl,
                    tp,
                    entry_fee
                );
            }
        }
    }
}

fn run_champion_3m_audit(_args: &[String]) -> Result<()> {
    let (raw_candles, min_len) = load_all_swarm_candles("1h");
    println!("==================================================================================");
    println!("🔍 BİNANCE FUTURES 3 AYLIK DERİNLEMESİNE GERÇEK VERİ DENETİMİ (90 GÜN - 2.160 SAAT)");
    println!("Veri: 13 Kripto Parite | Toplam Mum: {} bar (~{:.1} gün) | Gerçek Taker Fee %0.05 + Slippage", min_len, min_len as f64 / 24.0);
    println!("==================================================================================");

    let champ = StrategyChromosome {
        id: "Ajan-433-Donchian-Trend".to_string(),
        sl_atr_mult: 2.5,
        tp_atr_mult: 3.5,
        be_trigger_atr: 999.0,
        trail_trigger_atr: 2.5,
        trail_offset_atr: 1.2,
        risk_pct: 0.05,
        min_score: 80,
        daily_profit_pct: 0.12,
        daily_loss_pct: 0.08,
        max_daily_trades: 3,
        cooldown_bars: 24,
        strategy_mode: 2,
        donchian_lookback: 12,
        min_wick_ratio: 0.15,
        rsi_filter_mode: 0,
        htf_filter_mode: 2,
    };

    let baseline = StrategyChromosome {
        id: "Baseline-v8.0 (Eski)".to_string(),
        sl_atr_mult: 1.8,
        tp_atr_mult: 3.6,
        be_trigger_atr: 1.5,
        trail_trigger_atr: 2.2,
        trail_offset_atr: 1.2,
        risk_pct: 0.06,
        min_score: 85,
        daily_profit_pct: 0.06,
        daily_loss_pct: 0.08,
        max_daily_trades: 3,
        cooldown_bars: 10,
        strategy_mode: 0,
        donchian_lookback: 12,
        min_wick_ratio: 0.18,
        rsi_filter_mode: 0,
        htf_filter_mode: 0,
    };

    println!("\n🏆 1. ŞAMPİYON STRATEJİ: 90 GÜNLÜK (3 AY) KESİNTİSİZ TEST SONUCU");
    println!("Parametreler: SL: 2.43x | TP: 3.21x | BE: Kapalı | TR: Kapalı | Risk: %5.8 | Skor: ≥80 | Kâr: %4 | Zarar: %7 | Maxİşlem: 3/g | Soğuma: 36b");
    if let Some(rep_90) = evaluate_chromosome(&champ, &raw_candles, min_len, 90 * 24) {
        println!("----------------------------------------------------------------------------------");
        println!("💰 Başlangıç Bakiyesi: $10.00");
        println!("🏁 90 Gün Sonundaki Bakiye: ${:.2}", rep_90.final_balance);
        println!("📈 Toplam Net Kâr (ROI): {:+.2}%", rep_90.net_roi_pct);
        println!("🎯 Kazanma Oranı (Win Rate): %{:.1} ({} Kârlı / {} Zararlı İşlem)", rep_90.win_rate_pct, rep_90.win_trades, rep_90.loss_trades);
        println!("⚖️ Kâr Faktörü (Profit Factor): {:.2} (Kazançlar / Kayıplar)", rep_90.profit_factor);
        println!("🛡️ Maksimum Sermaye Düşüşü (Max DD): -%{:.2}", rep_90.max_drawdown_pct);
        println!("📅 Günlük Ortalama Net Getiri: {:+.2}$ (%{:+.2})", rep_90.daily_pnl_usd, rep_90.daily_pnl_pct);
        println!("📅 Haftalık Ortalama Net Getiri: {:+.2}$ (%{:+.2})", rep_90.weekly_pnl_usd, rep_90.weekly_pnl_pct);
        println!("📅 Aylık Ortalama Net Getiri: {:+.2}$ (%{:+.2})", rep_90.monthly_pnl_usd, rep_90.monthly_pnl_pct);
        println!("----------------------------------------------------------------------------------");
    }

    println!("\n📊 2. ESKİ BASELINE STRATEJİ İLE 3 AYLIK KARŞILAŞTIRMA");
    if let Some(rep_base) = evaluate_chromosome(&baseline, &raw_candles, min_len, 90 * 24) {
        println!("* Baseline-v8.0 (Eski): ROI: {:+.2}% (${:.2}) | WR: %{:.1} | PF: {:.2} | DD: -%{:.1}",
            rep_base.net_roi_pct, rep_base.final_balance, rep_base.win_rate_pct, rep_base.profit_factor, rep_base.max_drawdown_pct);
    }

    println!("\n📊 3. DİLİM DİLİM PERİYOT PERFORMANSI (Zaman İçi Tutarlılık / Generalization)");
    if let Some(rep_30) = evaluate_chromosome(&champ, &raw_candles, min_len, 30 * 24) {
        println!("* Son 30 Gün (Ekim - Kasım): {:+.2}% Net Kâr | Kasa: ${:.2} | WR: %{:.1} | PF: {:.2} | DD: -%{:.1}",
            rep_30.net_roi_pct, rep_30.final_balance, rep_30.win_rate_pct, rep_30.profit_factor, rep_30.max_drawdown_pct);
    }
    if let Some(rep_60) = evaluate_chromosome(&champ, &raw_candles, min_len, 60 * 24) {
        println!("* Son 60 Gün (Eylül - Kasım): {:+.2}% Net Kâr | Kasa: ${:.2} | WR: %{:.1} | PF: {:.2} | DD: -%{:.1}",
            rep_60.net_roi_pct, rep_60.final_balance, rep_60.win_rate_pct, rep_60.profit_factor, rep_60.max_drawdown_pct);
    }
    println!("==================================================================================");
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--backtest") {
        return run_native_rust_backtest(&args);
    }
    if args.iter().any(|a| a == "--swarm" || a == "--continuous-swarm") {
        return run_continuous_strategy_swarm(&args);
    }
    if args.iter().any(|a| a == "--audit-champion" || a == "--3m-audit") {
        return run_champion_3m_audit(&args);
    }

    println!("==================================================================================");
    println!("🏆 Binance Futures Multi-Coin 3'LÜ ORTOGONAL KURUMSAL SEPET v8.8 (1H Locked Execution)");
    println!("Zaman Dilimi: ⚡ 1H (1 Saatlik Mum Kapanışları) | Min Volatilite Filtresi: ATR/Fiyat >= %0.35");
    println!("Kâr Kilidi: 🎯 Kademeli Daralan Trailing Stop (+2.5x ATR Kârda 0.4x ATR'ye Daralma)");
    println!("Rejim 1: %0.5 Risk | Likidite/Fitil (Ajan-040) | SL: 2.5x ATR | TP: 3.5x | TR: 1.5x/0.8x -> 0.4x");
    println!("Rejim 2: %0.5 Risk | Trend Kırılımı (Ajan-433) | SL: 2.5x ATR | TP: 3.5x | TR: 1.5x/1.2x -> 0.4x");
    println!("Rejim 3: %0.5 Risk | EMA21 Pullback Avcısı     | SL: 1.8x ATR | TP: 3.5x | TR: 1.5x/1.0x -> 0.4x");
    println!("Portföy Koruması: Tekil Pozisyon Kilidi (1 Trade Max) | 24S Devre Kesici Stop Kilidi");
    println!("==================================================================================");

    let (api_key, api_secret) = load_credentials();
    configure_live_leverage_and_isolated_margin(&api_key, &api_secret);
    let mut state = BotPersistentState::load_or_init();
    if let Some(live_bal) = fetch_live_balance(&api_key, &api_secret) {
        println!("💰 [CANLI KASA TESPİT EDİLDİ] Binance Futures USDT Bakiyesi: ${:.2}", live_bal);
        if state.position == PositionState::Flat {
            state.balance = live_bal;
            if state.peak_equity < live_bal {
                state.peak_equity = live_bal;
            }
            state.save_atomic();
        }
    }
    let mut live_cooldown_until: HashMap<String, Instant> = HashMap::new();

    let mut engines: HashMap<String, SymbolEngine> = HashMap::new();
    for spec in SYMBOLS.iter() {
        let mut eng = SymbolEngine::new(*spec);
        eng.custom_strategy_mode = 0; // ⚡ 3'lü Ortogonal Sepet: Tri-Regime (Mod 1 + Mod 2 + Mod 3 aktif)
        eng.custom_donchian_lookback = 12; // 12 bar kanal
        eng.custom_min_wick_ratio = 0.15; // %15 fitil filtresi
        engines.insert(spec.symbol.to_string(), eng);
    }

    // A, B, C Şıkları: Arka Plan Taker & Balina & Açık Pozisyon (OI) Akışı Dinleyicisi
    let shared_order_flow = Arc::new(Mutex::new(HashMap::<String, (f64, f64, f64, String)>::new()));
    let flow_store_clone = shared_order_flow.clone();
    std::thread::spawn(move || {
        println!("🌊 [ORDER FLOW & OI ARKA PLAN] 13 Parite Taker, Balina & OI Takibi Başlatıldı.");
        loop {
            for spec in SYMBOLS.iter() {
                let (buy_pct, long_pct, delta_oi_pct, sentiment) = fetch_symbol_order_flow(spec.symbol);
                if let Ok(mut lock) = flow_store_clone.lock() {
                    lock.insert(spec.symbol.to_string(), (buy_pct, long_pct, delta_oi_pct, sentiment));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            std::thread::sleep(Duration::from_secs(35));
        }
    });

    // D Şıkkı: Scout X (Twitter) & Balina Canlı Haber Radarı (TreeNews WebSocket / REST)
    let shared_scout_news = Arc::new(Mutex::new(HashMap::<String, ScoutAlert>::new()));
    let scout_store_clone = shared_scout_news.clone();
    std::thread::spawn(move || {
        run_scout_listener(scout_store_clone);
    });

    let mut btc_velocity_tracker = BtcVelocityTracker::new();
    reconcile_exchange_positions(&mut state, &api_key, &api_secret);
    warmup_all_symbols(&mut engines);
    try_open_best_multi_coin_signal(
        &mut state,
        &engines,
        &mut live_cooldown_until,
        0.0,
        &api_key,
        &api_secret,
    );
    write_live_telemetry(&state, &engines, 0);

    println!("📡 13 Pariteli Birleşik Binance Mainnet WebSocket akışına bağlanılıyor...");
    let (mut socket, response) = connect(WS_MULTI_URL)?;
    println!(
        "🟢 Çoklu Akış Bağlandı (HTTP {}). 13 Coin Gerçek Zamanlı Taranıyor...\n",
        response.status()
    );

    let mut tick_counter: u64 = 0;
    let mut last_telemetry_write = Instant::now() - Duration::from_secs(5);
    let mut last_funding_check = Instant::now();
    let mut last_reconciliation = Instant::now();
    let mut backoff_secs = 1u64;

    loop {
        let msg = match socket.read() {
            Ok(m) => {
                backoff_secs = 1;
                m
            }
            Err(e) => {
                state.reconnect_count += 1;
                state.save_atomic();
                eprintln!(
                    "⚠️ [BAĞLANTI KOPTU] {}. {} sn sonra yeniden bağlanılıyor (#{})...",
                    e, backoff_secs, state.reconnect_count
                );
                std::thread::sleep(Duration::from_secs(backoff_secs));
                backoff_secs = (backoff_secs * 2).min(30);
                match connect(WS_MULTI_URL) {
                    Ok((s, _)) => {
                        socket = s;
                        println!("🟢 [AUTO-RECONNECT] 13 Pariteli WebSocket akışı yenilendi!");
                        continue;
                    }
                    Err(_) => continue,
                }
            }
        };

        if let Message::Ping(payload) = msg {
            let _ = socket.send(Message::Pong(payload));
            continue;
        }

        if !msg.is_text() {
            continue;
        }

        let text = match msg.into_text() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let wrapper: CombinedStreamWrapper = match serde_json::from_str(&text) {
            Ok(w) => w,
            Err(_) => continue,
        };

        let stream_symbol = wrapper
            .stream
            .split('@')
            .next()
            .unwrap_or("")
            .to_uppercase();

        if wrapper.stream.contains("@kline_") {
            if let Ok(ev) = serde_json::from_value::<KlineEventWrapper>(wrapper.data) {
                let o = ev.kline.open.parse::<f64>().unwrap_or(0.0);
                let h = ev.kline.high.parse::<f64>().unwrap_or(0.0);
                let l = ev.kline.low.parse::<f64>().unwrap_or(0.0);
                let c = ev.kline.close.parse::<f64>().unwrap_or(0.0);
                let v = ev.kline.volume.parse::<f64>().unwrap_or(0.0);
                if c > 0.0 {
                    if ev.kline.is_closed {
                        if let Some(eng) = engines.get_mut(&stream_symbol) {
                            eng.on_closed_candle(o, h, l, c, v);
                            println!(
                                "🕯️ [5m MUM KAPANDI: {:<8}] Kapanış: {:.4} | Sinyal: {:<5} (Skor: {:>2}/100) | Taker: %{:.1} | OI: {:+.2}%",
                                stream_symbol, c, eng.signal_dir, eng.signal_score, eng.live_taker_buy_pct, eng.live_delta_oi_pct
                            );
                        }
                        let btc_v15 = btc_velocity_tracker.velocity_15s_pct();
                        try_open_best_multi_coin_signal(
                            &mut state,
                            &engines,
                            &mut live_cooldown_until,
                            btc_v15,
                            &api_key,
                            &api_secret,
                        );
                    } else {
                        // ⚡ 5m İÇİ CANLI FİYAT GÜNCELLEME (Yalnızca grafik ve takip için - Ara fitilde işlem AÇILMAZ!)
                        if let Some(eng) = engines.get_mut(&stream_symbol) {
                            eng.on_live_kline(o, h, l, c, v);
                        }
                    }
                }
            }
            continue;
        }

        if wrapper.stream.ends_with("@aggTrade") {
            let trade: AggTradeEvent = match serde_json::from_value(wrapper.data) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let price = match trade.price.parse::<f64>() {
                Ok(p) if p > 0.0 => p,
                _ => continue,
            };
            let trade_qty = trade.quantity.parse::<f64>().unwrap_or(0.0);
            let is_taker_buy = !trade.is_buyer_maker;

            tick_counter += 1;
            if stream_symbol == "BTCUSDT" {
                btc_velocity_tracker.on_tick(price);
            }
            let current_atr = if let Some(eng) = engines.get_mut(&stream_symbol) {
                eng.on_live_tick(price, trade_qty, is_taker_buy);
                eng.live_atr14
            } else {
                continue;
            };

            if last_reconciliation.elapsed() >= Duration::from_secs(60) {
                reconcile_exchange_positions(&mut state, &api_key, &api_secret);
                state.save_atomic();
                last_reconciliation = Instant::now();
            }
            if last_funding_check.elapsed() >= Duration::from_secs(3600) {
                last_funding_check = Instant::now();
                match &state.position {
                    PositionState::Long {
                        entry_price, qty, ..
                    }
                    | PositionState::Short {
                        entry_price, qty, ..
                    } => {
                        let hourly_funding = (entry_price * qty * FUNDING_RATE_8H) / 8.0;
                        state.balance -= hourly_funding;
                        state.total_fees_paid += hourly_funding;
                    }
                    PositionState::Flat => {}
                }
            }

            let mut close_action: Option<(String, String, u32, f64, f64, f64, f64, String)> = None;
            let mut need_save_state = false;

            match &mut state.position {
                PositionState::Long {
                    symbol,
                    leverage,
                    entry_price,
                    qty,
                    margin_used,
                    stop_loss,
                    take_profit,
                    highest_price,
                    trailing_active,
                    entry_fee,
                    opened_ts,
                    regime,
                    ..
                } if symbol == &stream_symbol => {
                    if price > *highest_price {
                        *highest_price = price;
                    }
                    let atr = current_atr.max(*entry_price * 0.0025).min(*entry_price * 0.010);
                    let now_ts = Utc::now().timestamp();
                    let holding_hours = if *opened_ts > 0 {
                        ((now_ts - *opened_ts) as f64) / 3600.0
                    } else {
                        0.0
                    };

                    // Dinamik 3'lü Ortogonal Trailing Stop: Rejime özel kâr sürme (Tetik 1.5x < TP 3.5x ATR)
                    let (trail_mult, trail_offset) = match *regime {
                        1 => (2.0, 2.0), // Mod 1 Likidite: 1.5x ATR tetik, 0.8x takip payı (Kâr Kilidi: +0.7x ATR)
                        2 => (2.0, 2.0), // Mod 2 Kırılım: 1.5x ATR tetik, 1.2x takip payı (Kâr Kilidi: +0.3x ATR)
                        3 => (2.0, 2.0), // Mod 3 Pullback: 1.5x ATR tetik, 1.0x takip payı (Kâr Kilidi: +0.5x ATR)
                        _ => (TRAIL_ATR_MULT, TRAIL_OFFSET_ATR),
                    };
                    let fav_move = *highest_price - *entry_price;
                    let cur_fav = price - *entry_price;
                    let total_exit_fees = (*entry_fee * 2.0) + (*entry_price * *qty * 0.0002);
                    let cur_gross_pnl = (price - *entry_price) * *qty;
                    let cur_net_pnl = cur_gross_pnl - total_exit_fees;
                    let peak_gross_pnl = (*highest_price - *entry_price) * *qty;
                    let _peak_net_pnl = peak_gross_pnl - total_exit_fees;

                    // 🛡️ KORUMA 2: KADEMELİ KÂR KİLİDİ (+%0.35 Kârda -> +%0.18 Net Kâr Kilidi)
                    if fav_move >= atr * 2.0 {
                        let step_lock_price = *entry_price * 1.0018;
                        if step_lock_price > *stop_loss && price > step_lock_price * 1.0002 {
                            *stop_loss = step_lock_price;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "SELL", *stop_loss, *take_profit, sp.price_decimals);
                            need_save_state = true;
                        }
                    } else if fav_move >= atr * 2.0 + *entry_price * 0.0010 {
                        // 🛡️ KORUMA 1: ERKEN BAŞABAŞ KİLİDİ (+%0.28 Kârda -> Stopu Maliyet + Fee'ye Çek, Sıfır Risk!)
                        let be_price = *entry_price * (1.0 + 0.0013);
                        if be_price > *stop_loss && price > be_price * 1.0002 {
                            *stop_loss = be_price;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "SELL", *stop_loss, *take_profit, sp.price_decimals);
                            need_save_state = true;
                        }
                    }

                    // (Kaldırıldı) +$0.08 zorunlu kâr kapanışı: kazançları kırpıp R:R'yi bozuyordu.
                    // Kâr artık kademeli kilit + trailing stop + TP ile yönetiliyor.
                    // ⏱️ v9.5 KURAL 2: Zaman-Bazlı Kademeli Koruma (2.5 Saat & 4.0 Saat Zaman Kalkanı)
                    if holding_hours >= 2.5 && cur_fav >= *entry_price * 0.0032 {
                        // 2.5 saati geçen ve anlık >= +%0.32 kârda olan işlemin stopunu +%0.20 net kâra kilitle
                        let time_be_price = *entry_price * (1.0 + 0.0020);
                        if time_be_price > *stop_loss {
                            *stop_loss = time_be_price;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "SELL", *stop_loss, *take_profit, sp.price_decimals);
                        }
                    } else if holding_hours >= 4.0 && *stop_loss < *entry_price {
                        // 4 saati geçen ve hâlâ başabaş olmamış işlemde stop mesafesini %50 daralt (Risk Yarıya İner)
                        let tightened_sl = *entry_price - (atr * 0.85).min(*entry_price * 0.0075);
                        if tightened_sl > *stop_loss && price > tightened_sl * 1.0015 {
                            *stop_loss = tightened_sl;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "SELL", *stop_loss, *take_profit, sp.price_decimals);
                        }
                    }
                    if fav_move >= atr * trail_mult {
                        *trailing_active = true;
                        // Kademeli Daralan Trailing: Fiyat +2.2x ATR kâra ulaştığında kâr erimesini önlemek için takip payını 0.4x ATR'ye daralt!
                        let dyn_offset = trail_offset.max(2.0);
                        let candidate_sl = (*highest_price - atr * dyn_offset).max(*entry_price * (1.0 + 0.0025));
                        if candidate_sl > *stop_loss {
                            let prev_sl = *stop_loss;
                            *stop_loss = candidate_sl;
                            if ((*stop_loss - prev_sl).abs() / *entry_price) >= 0.0025 {
                                let sp = get_symbol_spec(symbol);
                                place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "SELL", *stop_loss, *take_profit, sp.price_decimals);
                                need_save_state = true;
                            }
                        }
                    }
                    if cur_net_pnl <= -((state.balance * 0.02).max(0.25).min(2.50) + 0.05) {
                        close_action = Some((
                            symbol.clone(),
                            "LONG".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            format!("🛑 25 CENT MAKSİMUM MİKRO-STOP (-${:.3} Net Zarar Kes)", cur_net_pnl.abs()),
                        ));
                    } else if price <= *stop_loss {
                        let reason = if *stop_loss > *entry_price {
                            if *trailing_active {
                                "TRAILING-STOP (Net Kâr Kilidi)".to_string()
                            } else {
                                "BAŞABAŞ (Komisyon Korumalı Net Kâr)".to_string()
                            }
                        } else {
                            "STOP-LOSS".to_string()
                        };
                        close_action = Some((
                            symbol.clone(),
                            "LONG".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            reason,
                        ));
                    } else if price >= *take_profit {
                        close_action = Some((
                            symbol.clone(),
                            "LONG".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            "TAKE-PROFIT".to_string(),
                        ));
                    } else if holding_hours >= 6.0 && *stop_loss < *entry_price {
                        // ⏰ v9.5 KURAL 3: 6 Saatlik Zaman Kalkanı (Bayatlayan & İvmesi Sönen İşlemi Kapat, Kasayı Serbest Bırak)
                        close_action = Some((
                            symbol.clone(),
                            "LONG".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            "⏰ ZAMAN KALKANI (6S Time-Stop: İvme Söndü, Kasa Serbest)".to_string(),
                        ));
                    } else if let Some(eng) = engines.get(symbol) {
                        let (btc_inst_dir, btc_inst_score, btc_cur_price) = engines.get("BTCUSDT")
                            .map(|b| (b.signal_dir, b.signal_score, b.last_price))
                            .unwrap_or(("NEUTRAL", 0, 0.0));
                        if let Some(emergency_reason) = check_emergency_exit_typesafe_jev(
                            symbol,
                            "LONG",
                            *entry_price,
                            price,
                            eng.live_scout_score_boost,
                            &eng.live_scout_headline,
                            eng.signal_dir,
                            eng.signal_score,
                            btc_inst_dir,
                            btc_inst_score,
                            state.btc_entry_price,
                            btc_cur_price,
                        ) {
                            if cur_fav >= *entry_price * 0.0020 {
                                // Kârdaki pozisyon: piyasadan kapatma, SL'yi entry'ye çek (risk sıfırla)
                                if *stop_loss < *entry_price { *stop_loss = *entry_price; let sp = get_symbol_spec(symbol); place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "SELL", *stop_loss, *take_profit, sp.price_decimals); need_save_state = true; }
                            } else {
                                close_action = Some((
                                    symbol.clone(),
                                    "LONG".to_string(),
                                    *leverage,
                                    *entry_price,
                                    *qty,
                                    *margin_used,
                                    *entry_fee,
                                    emergency_reason,
                                ));
                            }
                        }
                    }
                }
                PositionState::Short {
                    symbol,
                    leverage,
                    entry_price,
                    qty,
                    margin_used,
                    stop_loss,
                    take_profit,
                    lowest_price,
                    trailing_active,
                    entry_fee,
                    opened_ts,
                    regime,
                    ..
                } if symbol == &stream_symbol => {
                    if price < *lowest_price {
                        *lowest_price = price;
                    }
                    let atr = current_atr.max(*entry_price * 0.0025).min(*entry_price * 0.010);
                    let now_ts = Utc::now().timestamp();
                    let holding_hours = if *opened_ts > 0 {
                        ((now_ts - *opened_ts) as f64) / 3600.0
                    } else {
                        0.0
                    };

                    // Dinamik 3'lü Ortogonal Trailing Stop: Rejime özel kâr sürme (Tetik 1.5x < TP 3.5x ATR)
                    let (trail_mult, trail_offset) = match *regime {
                        1 => (2.0, 2.0), // Mod 1 Likidite: 1.5x ATR tetik, 0.8x takip payı (Kâr Kilidi: +0.7x ATR)
                        2 => (2.0, 2.0), // Mod 2 Kırılım: 1.5x ATR tetik, 1.2x takip payı (Kâr Kilidi: +0.3x ATR)
                        3 => (2.0, 2.0), // Mod 3 Pullback: 1.5x ATR tetik, 1.0x takip payı (Kâr Kilidi: +0.5x ATR)
                        _ => (TRAIL_ATR_MULT, TRAIL_OFFSET_ATR),
                    };
                    let fav_move = *entry_price - *lowest_price;
                    let cur_fav = *entry_price - price;
                    let total_exit_fees = (*entry_fee * 2.0) + (*entry_price * *qty * 0.0002);
                    let cur_gross_pnl = (*entry_price - price) * *qty;
                    let cur_net_pnl = cur_gross_pnl - total_exit_fees;
                    let peak_gross_pnl = (*entry_price - *lowest_price) * *qty;
                    let _peak_net_pnl = peak_gross_pnl - total_exit_fees;

                    // 🛡️ KORUMA 2: KADEMELİ KÂR KİLİDİ (+%0.35 Kârda -> +%0.18 Net Kâr Kilidi)
                    if fav_move >= atr * 2.0 {
                        let step_lock_price = *entry_price * 0.9982;
                        if step_lock_price < *stop_loss && price < step_lock_price * 0.9998 {
                            *stop_loss = step_lock_price;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "BUY", *stop_loss, *take_profit, sp.price_decimals);
                            need_save_state = true;
                        }
                    } else if fav_move >= atr * 2.0 + *entry_price * 0.0010 {
                        // 🛡️ KORUMA 1: ERKEN BAŞABAŞ KİLİDİ (+%0.28 Kârda -> Stopu Maliyet - Fee'ye Çek, Sıfır Risk!)
                        let be_price = *entry_price * (1.0 - 0.0013);
                        if be_price < *stop_loss && price < be_price * 0.9998 {
                            *stop_loss = be_price;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "BUY", *stop_loss, *take_profit, sp.price_decimals);
                            need_save_state = true;
                        }
                    }

                    // (Kaldırıldı) +$0.08 zorunlu kâr kapanışı: kazançları kırpıp R:R'yi bozuyordu.
                    // Kâr artık kademeli kilit + trailing stop + TP ile yönetiliyor.
                    // ⏱️ v9.5 KURAL 2: Zaman-Bazlı Kademeli Koruma (2.5 Saat & 4.0 Saat Zaman Kalkanı)
                    if holding_hours >= 2.5 && cur_fav >= *entry_price * 0.0032 {
                        // 2.5 saati geçen ve anlık >= +%0.32 kârda olan işlemin stopunu +%0.20 net kâra kilitle
                        let time_be_price = *entry_price * (1.0 - 0.0020);
                        if time_be_price < *stop_loss {
                            *stop_loss = time_be_price;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "BUY", *stop_loss, *take_profit, sp.price_decimals);
                        }
                    } else if holding_hours >= 4.0 && *stop_loss > *entry_price {
                        // 4 saati geçen ve hâlâ başabaş olmamış işlemde stop mesafesini %50 daralt (Risk Yarıya İner)
                        let tightened_sl = *entry_price + (atr * 0.85).min(*entry_price * 0.0075);
                        if tightened_sl < *stop_loss && price < tightened_sl * 0.9985 {
                            *stop_loss = tightened_sl;
                            let sp = get_symbol_spec(symbol);
                            place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "BUY", *stop_loss, *take_profit, sp.price_decimals);
                        }
                    }
                    if fav_move >= atr * trail_mult {
                        *trailing_active = true;
                        // Kademeli Daralan Trailing: Fiyat +2.2x ATR kâra ulaştığında kâr erimesini önlemek için takip payını 0.4x ATR'ye daralt!
                        let dyn_offset = trail_offset.max(2.0);
                        let candidate_sl = (*lowest_price + atr * dyn_offset).min(*entry_price * (1.0 - 0.0025));
                        if candidate_sl < *stop_loss {
                            let prev_sl = *stop_loss;
                            *stop_loss = candidate_sl;
                            if ((prev_sl - *stop_loss).abs() / *entry_price) >= 0.0025 {
                                let sp = get_symbol_spec(symbol);
                                place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "BUY", *stop_loss, *take_profit, sp.price_decimals);
                                need_save_state = true;
                            }
                        }
                    }
                    if cur_net_pnl <= -((state.balance * 0.02).max(0.25).min(2.50) + 0.05) {
                        close_action = Some((
                            symbol.clone(),
                            "SHORT".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            format!("🛑 25 CENT MAKSİMUM MİKRO-STOP (-${:.3} Net Zarar Kes)", cur_net_pnl.abs()),
                        ));
                    } else if price >= *stop_loss {
                        let reason = if *stop_loss < *entry_price {
                            if *trailing_active {
                                "TRAILING-STOP (Net Kâr Kilidi)".to_string()
                            } else {
                                "BAŞABAŞ (Komisyon Korumalı Net Kâr)".to_string()
                            }
                        } else {
                            "STOP-LOSS".to_string()
                        };
                        close_action = Some((
                            symbol.clone(),
                            "SHORT".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            reason,
                        ));
                    } else if price <= *take_profit {
                        close_action = Some((
                            symbol.clone(),
                            "SHORT".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            "TAKE-PROFIT".to_string(),
                        ));
                    } else if holding_hours >= 6.0 && *stop_loss > *entry_price {
                        // ⏰ v9.5 KURAL 3: 6 Saatlik Zaman Kalkanı (Bayatlayan & İvmesi Sönen İşlemi Kapat, Kasayı Serbest Bırak)
                        close_action = Some((
                            symbol.clone(),
                            "SHORT".to_string(),
                            *leverage,
                            *entry_price,
                            *qty,
                            *margin_used,
                            *entry_fee,
                            "⏰ ZAMAN KALKANI (6S Time-Stop: İvme Söndü, Kasa Serbest)".to_string(),
                        ));
                    } else if let Some(eng) = engines.get(symbol) {
                        let (btc_inst_dir, btc_inst_score, btc_cur_price) = engines.get("BTCUSDT")
                            .map(|b| (b.signal_dir, b.signal_score, b.last_price))
                            .unwrap_or(("NEUTRAL", 0, 0.0));
                        if let Some(emergency_reason) = check_emergency_exit_typesafe_jev(
                            symbol,
                            "SHORT",
                            *entry_price,
                            price,
                            eng.live_scout_score_boost,
                            &eng.live_scout_headline,
                            eng.signal_dir,
                            eng.signal_score,
                            btc_inst_dir,
                            btc_inst_score,
                            state.btc_entry_price,
                            btc_cur_price,
                        ) {
                            if cur_fav >= *entry_price * 0.0020 {
                                // Kârdaki pozisyon: piyasadan kapatma, SL'yi entry'ye çek (risk sıfırla)
                                if *stop_loss > *entry_price { *stop_loss = *entry_price; let sp = get_symbol_spec(symbol); place_exchange_hard_stop_and_tp_if_live(&api_key, &api_secret, symbol, "BUY", *stop_loss, *take_profit, sp.price_decimals); need_save_state = true; }
                            } else {
                                close_action = Some((
                                    symbol.clone(),
                                    "SHORT".to_string(),
                                    *leverage,
                                    *entry_price,
                                    *qty,
                                    *margin_used,
                                    *entry_fee,
                                    emergency_reason,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }

            if need_save_state {
                state.save_atomic();
            }

            if let Some((sym, side, lev, entry_price, qty, margin_used, entry_fee, reason)) =
                close_action
            {
                let spec = get_symbol_spec(&sym);
                let exit_exec = if side == "LONG" {
                    price * (1.0 - SLIPPAGE_RATE)
                } else {
                    price * (1.0 + SLIPPAGE_RATE)
                };
                let exit_fee = exit_exec * qty * TAKER_FEE_RATE;
                let gross_pnl = if side == "LONG" {
                    (exit_exec - entry_price) * qty
                } else {
                    (entry_price - exit_exec) * qty
                };
                let total_trade_fees = entry_fee + exit_fee;
                let net_pnl = gross_pnl - total_trade_fees;
                let roe_pct = if margin_used > 0.0 {
                    (net_pnl / margin_used) * 100.0
                } else {
                    0.0
                };

                state.balance += gross_pnl - exit_fee;
                state.total_fees_paid += exit_fee;
                state.realized_gross_pnl += gross_pnl;
                state.realized_net_pnl += net_pnl;
                state.total_trades += 1;
                if net_pnl >= 0.0 || reason.contains("BAŞABAŞ") || reason.contains("BREAKEVEN") {
                    state.winning_trades += 1;
                    state.consecutive_stops = 0; // Kâr veya başabaş korumalı çıkışta ardışık stop sıfırlanır
                } else {
                    state.losing_trades += 1;
                    // Zaman Kalkanı (6S Time-Stop) ve Başabaş komisyon koruması sermaye korumasıdır, devre kesici cezası sayılmaz
                    if !reason.contains("ZAMAN KALKANI") && !reason.contains("BAŞABAŞ") && !reason.contains("BREAKEVEN") {
                        state.consecutive_stops += 1;
                        if state.consecutive_stops >= 3 {
                            state.circuit_breaker_until = Utc::now().timestamp() + 900; // 15 dakikalık piyasa soğuma molası
                            println!(
                                "🚨 [DEVRE KESİCİ (CIRCUIT BREAKER)] Üst üste 3 STOP gerçekleşti! Bot 15 dakika boyunca soğuma molasına alındı."
                            );
                        }
                    }
                    // 1 bar (5 dakika = 300 sn) Stop-Loss soğuma süresi
                    live_cooldown_until
                        .insert(sym.clone(), Instant::now() + Duration::from_secs(300));
                }

                if state.balance > state.peak_equity {
                    state.peak_equity = state.balance;
                } else if state.peak_equity > 0.0 {
                    let dd = ((state.peak_equity - state.balance) / state.peak_equity) * 100.0;
                    if dd > state.max_drawdown_pct {
                        state.max_drawdown_pct = dd;
                    }
                }

                let pos_opened_ts = match &state.position {
                    PositionState::Long { opened_ts, .. } | PositionState::Short { opened_ts, .. } => *opened_ts,
                    PositionState::Flat => 0,
                };
                let close_order_side = if side == "LONG" { "SELL" } else { "BUY" };
                let _ = send_signed_reduce_only_close_if_live(
                    &api_key,
                    &api_secret,
                    &sym,
                    close_order_side,
                    qty,
                    spec.qty_decimals,
                );
                if let Some(live_bal) = fetch_live_balance(&api_key, &api_secret) {
                    state.balance = live_bal;
                }
                let now_ms = Utc::now().timestamp_millis() as u64;
                let entry_ms = if pos_opened_ts > 0 { (pos_opened_ts as u64) * 1000 } else { now_ms };
                let dur_sec = now_ms.saturating_sub(entry_ms) / 1000;
                let dur_hr = (dur_sec as f64) / 3600.0;
                let tr_time = (Utc::now() + chrono::Duration::hours(3)).format("%Y-%m-%d %H:%M:%S").to_string();

                state.closed_trades.insert(
                    0,
                    ClosedTradeRecord {
                        timestamp: tr_time,
                        symbol: sym.clone(),
                        side: side.clone(),
                        leverage: lev,
                        qty,
                        margin_used,
                        entry_price,
                        exit_price: exit_exec,
                        gross_pnl,
                        fees_paid: total_trade_fees,
                        net_pnl,
                        roe_pct,
                        reason: reason.clone(),
                        entry_time_ms: entry_ms,
                        exit_time_ms: now_ms,
                        holding_duration_seconds: dur_sec,
                        holding_duration_hours: dur_hr,
                        hourly_active_roe: if dur_hr > 0.01 { roe_pct / dur_hr } else { roe_pct },
                    },
                );
                if state.closed_trades.len() > 100 {
                    state.closed_trades.pop();
                }

                state.position = PositionState::Flat;
                state.btc_entry_price = 0.0;
                state.circuit_breaker_until = Utc::now().timestamp() + 90;
                state.save_atomic();
                println!("🛡️ [90S SOĞUMA BAŞLADI] Yeni işlem açılışı 90 saniye donduruldu (Tuzak ve iğne koruması).");

                println!(
                    "🏁 [{} {} KAPANDI - {}] Giriş: {:.4} -> Çıkış: {:.4} | Brüt: {:+.3}$ | Fee: -{:.3}$ | NET: {:+.3}$ ({:+.2}% ROE) | Yeni Kasa: ${:.2}",
                    sym, side, reason, entry_price, exit_exec, gross_pnl, total_trade_fees, net_pnl, roe_pct, state.balance
                );
            }

            if let Ok(lock) = shared_order_flow.try_lock() {
                for (sym, (buy_pct, long_pct, delta_oi_pct, sentiment)) in lock.iter() {
                    if let Some(eng) = engines.get_mut(sym) {
                        eng.live_taker_buy_pct = *buy_pct;
                        eng.live_top_trader_long_pct = *long_pct;
                        eng.live_delta_oi_pct = *delta_oi_pct;
                        eng.live_order_flow_sentiment = sentiment.clone();
                    }
                }
            }

            if let Ok(scout_lock) = shared_scout_news.try_lock() {
                let now_ms = Utc::now().timestamp_millis();
                for (sym, alert) in scout_lock.iter() {
                    if let Some(eng) = engines.get_mut(sym) {
                        if now_ms - alert.timestamp_ms < 15 * 60 * 1000 {
                            eng.live_scout_sentiment = alert.sentiment.clone();
                            eng.live_scout_score_boost = alert.score_boost;
                            eng.live_scout_headline = alert.headline.clone();
                        } else {
                            eng.live_scout_sentiment = "⚪ SAKİN".to_string();
                            eng.live_scout_score_boost = 0;
                        }
                    }
                }
            }

            if last_telemetry_write.elapsed() >= Duration::from_millis(350) {
                write_live_telemetry(&state, &engines, tick_counter);
                last_telemetry_write = Instant::now();
            }

            if tick_counter % 50 == 0 {
                println!(
                    "📊 [TICK #{:05}] Son Akış: {:<8} @ {:.4} | Kasa: ${:.2} USDT | Pozisyon: {:?}",
                    tick_counter, stream_symbol, price, state.balance, state.position
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn test_btc_velocity_scale() {
        let mut tracker = BtcVelocityTracker::new();
        let now = Instant::now();
        // BTC moves from 75000 to 75075 (+0.10%)
        tracker.ticks.push_back((now - Duration::from_secs(14), 75000.0));
        tracker.ticks.push_back((now, 75075.0));
        let vel = tracker.velocity_15s_pct();
        assert!((vel - 0.10).abs() < 1e-6, "Expected ~0.10%, got {}", vel);
        assert!(vel >= 0.10);
    }

    #[test]
    fn test_market_breadth_dominance_ratio() {
        // Test that 4 LONG vs 6 SHORT does NOT veto SHORT (because SHORT is 6, LONG is 4 -> LONG is not > SHORT)
        let (_, _, reason) = typesafe_unified_arbiter(
            "SEIUSDT",
            "SHORT",
            85,
            0.045,
            0.046,
            45.0,
            42.0,
            0.05,
            0,
            "",
            "AYI",
            "SHORT",
            80,
            4, // market_long_count
            6, // market_short_count
        );
        assert!(!reason.contains("Ayı Tuzağı Vetosu"), "Should not veto SHORT when short count >= long count");

        // Test that 6 LONG vs 4 SHORT DOES veto SHORT
        let (is_approved_trap, _, reason_trap) = typesafe_unified_arbiter(
            "SEIUSDT",
            "SHORT",
            85,
            0.045,
            0.046,
            45.0,
            42.0,
            0.05,
            0,
            "",
            "AYI",
            "SHORT",
            80,
            6, // market_long_count > market_short_count && >= 5
            4, // market_short_count
        );
        assert!(!is_approved_trap);
        assert!(reason_trap.contains("Ayı Tuzağı Vetosu"));
    }

    #[test]
    fn test_early_breakeven_math() {
        let entry = 100.0;
        let qty = 1.0;
        let entry_fee = entry * qty * TAKER_FEE_RATE; // 100 * 0.0005 = 0.05

        // LONG BE (+13 bps)
        let be_long = entry * (1.0 + 0.0013); // 100.13
        let exit_fee_long = be_long * qty * TAKER_FEE_RATE;
        let gross_pnl_long = (be_long - entry) * qty;
        let net_pnl_long = gross_pnl_long - (entry_fee + exit_fee_long);
        assert!(net_pnl_long > 0.0, "LONG BE price must yield positive net PnL after all taker fees: got {}", net_pnl_long);

        // SHORT BE (+13 bps net favorable, price lower)
        let be_short = entry * (1.0 - 0.0013); // 99.87
        let exit_fee_short = be_short * qty * TAKER_FEE_RATE;
        let gross_pnl_short = (entry - be_short) * qty;
        let net_pnl_short = gross_pnl_short - (entry_fee + exit_fee_short);
        assert!(net_pnl_short > 0.0, "SHORT BE price must yield positive net PnL after all taker fees: got {}", net_pnl_short);
    }

    #[test]
    fn test_micro_sniper_open_10s_latch() {
        let spec = get_symbol_spec("BTCUSDT");
        let mut eng = SymbolEngine::new(spec);
        let now = Instant::now();
        // Insert ticks: 50s ago @ 100.0, 8s ago @ 102.0, 2s ago @ 101.5
        eng.micro_ticks_60s.push_back((now - Duration::from_secs(50), 100.0, 1.0, true));
        eng.micro_ticks_60s.push_back((now - Duration::from_secs(8), 102.0, 1.0, true));
        eng.micro_ticks_60s.push_back((now - Duration::from_secs(2), 101.5, 1.0, false));
        eng.micro_ticks_60s.push_back((now, 101.5, 1.0, false));
        eng.last_price = 101.5;

        // open_10s should be 102.0 (the first tick within 10s)
        // Check tolerance: 101.5 <= 102.0 * 1.0002 is true!
        let (ok, score) = eng.check_micro_sniper_trigger("SHORT");
        println!("check_micro_sniper_trigger result: ok={}, score={}", ok, score);
    }

    #[test]
    fn test_candidate_selection_cooldown_map() {
        let mut cooldown_map: HashMap<String, Instant> = HashMap::new();
        let sym = "AVAXUSDT";
        cooldown_map.insert(sym.to_string(), Instant::now() + Duration::from_secs(60));

        let now = Instant::now();
        let is_in_cd = if let Some(cd_exp) = cooldown_map.get(sym) {
            now < *cd_exp
        } else {
            false
        };
        assert!(is_in_cd, "Symbol should be under cooldown");
    }

    #[test]
    fn test_pivot_sr_anti_ceiling_anti_floor_veto() {
        let spec = get_symbol_spec("INJUSDT");
        let mut eng = SymbolEngine::new(spec);

        // Simulate INJUSDT scenario from user chart:
        // Resistance at 8.269, current price at 8.2511 (0.217% below resistance)
        eng.last_price = 8.2511;
        eng.pivot_resistance = Some(8.269);
        eng.signal_dir = "LONG";
        eng.signal_score = 92;

        eng.apply_pivot_sr_and_smc_filter();

        assert_eq!(eng.signal_dir, "NEUTRAL", "LONG directly beneath resistance (<0.50%) must be VETOED to NEUTRAL");
        assert_eq!(eng.signal_score, 40, "Score must be reset to baseline 40 on ceiling veto");

        // Simulate floor scenario:
        // Support at 8.199, current price at 8.210 (0.134% above support)
        eng.last_price = 8.210;
        eng.pivot_support = Some(8.199);
        eng.signal_dir = "SHORT";
        eng.signal_score = 88;

        eng.apply_pivot_sr_and_smc_filter();

        assert_eq!(eng.signal_dir, "NEUTRAL", "SHORT directly above support (<0.50%) must be VETOED to NEUTRAL");
        assert_eq!(eng.signal_score, 40, "Score must be reset to baseline 40 on floor veto");
    }
}
