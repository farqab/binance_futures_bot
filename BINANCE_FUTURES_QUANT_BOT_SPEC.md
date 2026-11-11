# 🏛️ BINANCE FUTURES MULTI-COIN RUST QUANT BOT: MİMARİ, MATEMATİK VE RİSK YÖNETİMİ TEKNİK ŞARTNAMESİ (SPECIFICATION)
**Sürüm:** v10.2 (Tri-Regime Scalp & Confluence Sniper Architecture)  
**Tarih:** 10 Kasım 2026  
**Hedef Kitle:** Yapay Zeka Denetçileri (Claude 3.7 Sonnet, DeepSeek R1, OpenAI o1/o3), Kantitatif Araştırmacılar, Finansal Yazılım Mühendisleri.  
**Platform:** Binance USDT-M Perpetual Futures Mainnet  
**Çalışma Ortamı:** AWS EC2 (Ubuntu 24.04 LTS, Systemd Daemon, Sıfır-Gecikmeli Rust Binary)

---

## 1. YÖNETİCİ ÖZETİ VE SİSTEMİN TEMEL AMACI

Bu belge; Binance Vadeli İşlemler (USDT-M Perpetual Futures) piyasasında mikro-sermaye ölçeğinde ($10 - $1000) çalışan, **sıfır-çöp toplayıcı (zero-GC) gecikmesine sahip saf Rust diliyle geliştirilmiş**, çoklu zaman dilimli (Multi-Timeframe), sipariş akışı (Order Flow) ve likidite mikroyapısına dayalı otonom algoritmik alım-satım sisteminin eksiksiz teknik mimarisini açıklar.

### Temel Sistem Hedefleri:
1. **Sermaye Koruma Önceliği (Capital Preservation First):** Asimetrik risk/ödül oranı (minimum 1:2.5, ortalama 1:3.5).
2. **Korelasyon Riskinin Sıfırlanması:** Aynı anda yalnızca **TEK BİR AKTİF POZİSYON (1 Single Trade Max)** açılır. Portföyde birden fazla altcoin pozisyonu açılarak piyasa şelalesinde çoklu tasfiye (cascading liquidation) riski alınmaz.
3. **Milisaniye Seviyesinde İcra:** Python veya Node.js gibi dillerin getirdiği Garbage Collection (GC) duraksamaları elenmiş; tüm veri yapıları Rust `Arc<Mutex<T>>`, `VecDeque` ve `HashMap` ile optimize edilmiştir.
4. **Donanım Seviyesinde Güvenlik:** İşlem açıldığı anda Binance eşleşme motoruna (Matching Engine) sunucu tarafında `STOP_MARKET` ve `TAKE_PROFIT_MARKET` emirleri yerleştirilir. Elektrik, internet veya sunucu çökse dahi borsa tarafındaki stop emri sermayeyi korur.

---

## 2. DONANIM, AĞ VE YAZILIM MİMARİSİ

```
                                      ┌──────────────────────────────────────────────┐
                                      │        BINANCE FUTURES MAINNET               │
                                      │  (Multi-Stream WebSocket + REST API v1)      │
                                      └──────────────────────┬───────────────────────┘
                                                             │
                                        wss://fstream.binance.com/stream
                                        (30 Parite AggTrade + 5m Klines)
                                                             │
                                                             ▼
┌────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                             AWS EC2 RUST QUANT CORE DAEMON                                             │
│                                                                                                                        │
│   ┌────────────────────────────────────────────────┐                 ┌─────────────────────────────────────────────┐   │
│   │             TICK & CANDLE INGESTION            │                 │              BTC QUAD-TF ENGINE             │   │
│   │  • Atomic circular buffers (VecDeque)          │                 │  • 1m, 5m, 15m, 1h SMA(50) + Slopes         │   │
│   │  • Micro-velocity tracking (15s btc_vel)       │                 │  • is_scalp_bullish / is_scalp_bearish      │   │
│   └───────────────────────┬────────────────────────┘                 └──────────────────────┬──────────────────────┘   │
│                           │                                                                 │                          │
│                           ▼                                                                 ▼                          │
│   ┌────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐   │
│   │                              MULTI-COIN SYMBOL ENGINES (13 Aktif Likit Parite)                                 │   │
│   │   BTC, ETH, SOL, BNB, DOGE, NEAR, APT, SUI, INJ, AVAX, LINK, ARB, 1000PEPE                                    │   │
│   │                                                                                                                │   │
│   │   [REJİM 1: FİTİL SÜPÜRME]           [REJİM 2: TREND KIRILIMI]          [REJİM 3: EMA PULLBACK AVCISI]         │   │
│   │   Likidite Ret & Confluence          Donchian 12-Bar Breakout           Dinamik EMA21 Trend Testi              │   │
│   │   Skor: 75-99 | Ajan-040             Skor: 72-99 | Ajan-433             Skor: 74-99 (Min Eşik Kalkanı)         │   │
│   └───────────────────────────────────────────────────────┬────────────────────────────────────────────────────────┘   │
│                                                           │                                                            │
│                                                           ▼                                                            │
│   ┌────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐   │
│   │                                     GÜVENLİK VE FİLTRELEME KALKANLARI                                          │   │
│   │  1. Zero-Lag BTC Gate (Zıt yönde işlem yasağı)        4. Tavan/Taban Tuzağı Veto (15 bps tampon)               │   │
│   │  2. Piyasa Konsensüsü (Market Breadth >= 6)           5. Haber/Balina Scout Radar (-40 Puan Veto)              │   │
│   │  3. Pine Script Dinamik Bant Kontrolü                 6. 30 Dakika Stop-Loss Cooldown                          │   │
│   └───────────────────────────────────────────────────────┬────────────────────────────────────────────────────────┘   │
│                                                           │                                                            │
│                                                           ▼                                                            │
│   ┌────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐   │
│   │                                     POZİSYON VE RİSK YÖNETİM MOTORU                                            │   │
│   │  • 5x ISOLATED Marjin                                 • Dinamik Kademeli Trailing Stop                         │   │
│   │  • 25 Cent Hard Stop Bütçesi                          • 6S Zaman Kalkanı (Time-Stop)                           │   │
│   │  • Erken Başabaş Kilidi (+%0.28 kârda BE lock)        • Devre Kesici (3 ardışık stopta 15 dk mola)             │   │
│   └────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. MAKRO PİYASA VE BTC ÇAPASI: BTC QUAD-TF ENGINE

Kripto para piyasasında altcoinlerin %95'i Bitcoin'in yönüne bağımlıdır. Bot, her 10 saniyede bir arka planda çalışan izole bir iş parçacığında (thread) Bitcoin'in 4 farklı zaman dilimini hesaplar:

### Zaman Dilimleri ve Formüller:
1. **1 Dakikalık (1m) SMA(50) + Eğim (Slope 1m):** Anlık mikro momentum.
2. **5 Dakikalık (5m) SMA(50) + Eğim (Slope 5m):** Taktiksel scalp trendi.
3. **15 Dakikalık (15m) SMA(50) + Eğim (Slope 15m):** Kısa vadeli kurumsal yapı.
4. **1 Saatlik (1h) SMA(50) + Eğim (Slope 1h):** Makro trend filtresi.

### Seçenek 1: Scalp Mutabakatı (Dengeli Mod Kuralı)
Geçmişte uygulanan 4 zaman diliminin birden aynı yöne bakması (1h dahil) kuralı, 1 saatlik ortalamanın gecikmesi (lag) nedeniyle kârlı scalp dalgalarını kaçırıyordu. Sistem şu kurala optimize edilmiştir:

$$\text{is\_scalp\_bullish} = (P_{BTC} > \text{SMA}_{1m}) \land (P_{BTC} > \text{SMA}_{5m}) \land (P_{BTC} > \text{SMA}_{15m}) \land (\text{Slope}_{5m} \ge -0.05)$$

$$\text{is\_scalp\_bearish} = (P_{BTC} < \text{SMA}_{1m}) \land (P_{BTC} < \text{SMA}_{5m}) \land (P_{BTC} < \text{SMA}_{15m}) \land (\text{Slope}_{5m} \le +0.05)$$

* **Demir Kural:** Eğer BTC Scalp Boğa ise (`is_scalp_bullish`), altcoinlerde **SHORT AÇILMASI KESİNLİKLE YASAKTIR**.
* Eğer BTC Scalp Ayı ise (`is_scalp_bearish`), altcoinlerde **LONG AÇILMASI KESİNLİKLE YASAKTIR**.
* Eğer zaman dilimleri çatışıyorsa, sistem **TAM NAKİTTE (FLAT)** bekler.

---

## 4. 3'LÜ ORTOGONAL STRATEJİ SEPETİ (TRI-REGIME QUANT ENGINE)

Piyasa koşulları tek bir stratejiyle yenilemez (No Free Lunch Theorem). Sistem, birbirine dik (orthogonal) çalışan 3 farklı pazar rejimini aynı anda tarar:

### REJİM 1: Likidite ve Fitil Süpürme (Pro-Trend Shakeout / Ajan-040)
* **Piyasa Felsefesi:** Fiyat trend yönündeyken, ters yönde stop avı (liquidity sweep) yapan ve ardından sertçe geri dönen mumları avlar.
* **Koşullar (SHORT için):**
  1. $High > High_{12}$ ve $Close < High_{12}$ (12 mumun zirvesi süpürülüp altına inildi).
  2. $\frac{\text{Upper Wick}}{\text{Total Range}} \ge 0.18$ (Belirgin üst satış fitili).
  3. $Close < Open$ (Kırmızı gövde kapanışı zorunludur; yeşil kırılım mumlarında asla tetiklenmez).
  4. $\text{EMA}_9 \le \text{EMA}_{21} \times 1.002 \land \text{EMA}_{21} \le \text{EMA}_{50} \times 1.002$.
* **Skor Formülü:**
  $$\text{Skor} = 75 + \min\left(15, \frac{\text{Upper Wick}}{\text{Total Range}} \times 25\right) + \text{TakerBonus} + \text{DeltaOIBonus} + \text{TopTraderBonus}$$
* **Confluence Sniper Eklentisi:** Eğer 5m SMA(50) ile Pivot Direnç seviyesi `%0.25` yakınlıkta kesişmişse ve fiyat buradan ret yemişse, skora anında **+10 Ekstra Puan** eklenir. Mavi tabanın kırılması beklenmeden en tepeden girilir.

### REJİM 2: Momentum & Donchian Trend Kırılımı (Breakout / Ajan-433)
* **Piyasa Felsefesi:** Sıkışan konsolidasyon bantlarının hacimle patlamasını takip eder.
* **Koşullar (LONG için):**
  1. $Close \ge High_{12} \land Close > Open$.
  2. $\text{EMA}_9 \ge \text{EMA}_{21} \times 0.998$.
  3. $46.0 \le \text{RSI}_{14} \le 62.0$ (Tükeniş bölgesinde olmamalı).
  4. **Turuncu Tavan Şartı:** Fiyat Pine Script dinamik direnç tavanının üzerinde olmalıdır ($Close \ge Res \times 0.9995$).
* **Skor:** Taban 72 + Sipariş akışı bonusları (Max 99).

### REJİM 3: Dinamik EMA21 Pullback Avcısı
* **Piyasa Felsefesi:** Güçlü trendlerde aşırı uzamış fiyata atlamak yerine EMA21 dinamik destek/direncine yapılan sağlıklı geri çekilmeleri yakalar.
* **Koşullar (SHORT için):**
  1. $\text{EMA}_9 < \text{EMA}_{21}$.
  2. $High \ge \text{EMA}_{21} \times 0.9975 \land Close < \text{EMA}_{21}$.
  3. $Close < Open \land \frac{\text{Upper Wick}}{\text{Total Range}} \ge 0.15$.
  4. $32.0 \le \text{RSI}_{14} \le 54.0$.
* **Testere Piyasası Kalkanı (Audit Eklentisi):** Rejim 3 sinyalleri, yatay piyasa testeresinde sahte tepkileri elemek için **en az 74 Sinyal Skoruna** sahip olmak zorundadır. 74 altındaki tereddütlü geri çekilmeler doğrudan elenir.

---

## 5. MİKROYAPI VE SİPARİŞ AKIŞI (ORDER FLOW INTELLIGENCE)

Bot, klasik indikatörlerin gecikmesini aşmak için doğrudan emir defteri mikroyapısını okur:

| Değişken | Açıklama | Sinyal Katkısı |
| :--- | :--- | :--- |
| `live_taker_buy_pct` | Son 5 dakikadaki piyasa (taker) alım hacminin toplam hacme oranı. | $>\%52$ ise LONG (+8), $<\%48$ ise SHORT (+8) |
| `live_delta_oi_pct` | Son 5 dakikadaki Açık Pozisyon (Open Interest) yüzdesel değişimi. | $\ge +\%0.05$ kurumsal katılım teyidi (+7) |
| `live_top_trader_long_pct` | Binance Lider Trader (Top Trader) hesaplarının Long oranı. | Akıllı para pozisyonu ile uyum (+5) |
| `live_scout_score_boost` | TreeNews WebSocket üzerinden gelen canlı haber ve X (Twitter) analizi. | Kötü haber/açık durumunda **-40 Puan VETO** |

---

## 6. GÜVENLİK KALKANLARI VE RİSK YÖNETİMİ

### 1. Tavan ve Taban Tuzağı Kalkanı (Anti-Ceiling & Anti-Floor Firewall)
* **Problem:** Fiyatın hemen üzerinde direnç varken LONG açmak veya desteğin hemen üzerinde SHORT açmak kurumsal tuzaklara düşürür.
* **Matematiksel Çözüm:** Fiyat ile Pivot Direnç arasındaki mesafe `%0.15` (15 baz puan) altındaysa LONG açılması **KESİNLİKLE VETOLANIR**. Desteğe `%0.15` mesafede SHORT açılması yasaktır.

### 2. Erken Başabaş Kilidi (Early Breakeven Lock)
* İşlem açıldıktan sonra lehte hareket $+\%0.28$ seviyesine ulaştığında, stop seviyesi derhal borsa komisyonunu ($\%0.04 \times 2 = \%0.08$) ve net kârı garanti eden seviyeye çekilir:
  $$\text{Stop}_{\text{SHORT}} = \text{Entry} \times (1.0 - 0.0013)$$
* Piyasa ani bir fitille tersine dönse dahi işlem pozitif kârla kapatılır.

### 3. Kademeli Daralan Trailing Stop (Dynamic Trailing Stop)
* Kâr $+2.2\times \text{ATR}$ seviyesine ulaştığında kâr erimesini önlemek için takip payı $0.70\times \text{ATR}$'den **$0.40\times \text{ATR}$'ye daraltılır**.

### 4. 25 Cent Hard Dolar Risk Bütçesi
* Her ne olursa olsun, bir işlemde kaybedilebilecek maksimum net zarar strictly **$0.25 USDT** ile sınırlandırılmıştır. Pozisyon büyüklüğü bu bütçeye göre dinamik hesaplanır.

### 5. 30 Dakika Stop-Loss Soğuma Süresi (Whipsaw Shield)
* Bir coin stop-loss ile kapandığında, bot o coine **1800 saniye (30 dakika)** boyunca yeni işlem açamaz. Testere piyasasında aynı coin üzerinde peş peşe doğranma riski sıfırlanmıştır.

### 6. Devre Kesici (Circuit Breaker)
* Üst üste 3 kez stop-loss gerçekleşirse sistem kendini **15 dakika** boyunca tamamen kilitler.
* Kasa zirve bakiyeden (Peak Equity) $\%15$ çekilme yaşarsa sistem **1 saatlik koruma uykusuna** geçer.

---

## 7. AMPİRİK BACKTEST VE CANLI MAINNET PERFORMANSI

### 5 Günlük Gerçek Veri Backtesti (5-9 Kasım 2026, 15 Parite, 1440 Mum):
* **Toplam İşlem Sayısı:** 31 (Filtrelenmiş kaliteli sinyaller)
* **Kazanma Oranı (Win Rate):** **%71.0** (22 Kazanç / 9 Kayıp)
* **Brüt Kâr:** +$2.68 USDT
* **Ödenen Binance Taker Komisyonu:** -$1.15 USDT
* **Net Kasa Büyümesi:** **+$1.53 USDT (+%16.2 Net Kâr)**

### Canlı Mainnet İcra Kanıtı (10 Kasım 2026):
* **Son İşlem:** `1000PEPEUSDT` SHORT (Giriş: 0.0052717, Çıkış: 0.0052649)
* **Süre:** 48 Saniye
* **Sonuç:** `BAŞABAŞ (Komisyon Korumalı Net Kâr)`
* **Bakiye Değişimi:** **$9.44 USDT ➡️ $9.49 USDT (+%0.53 büyüme)**

---

## 8. HARİCİ YAPAY ZEKALAR VE UZMANLAR İÇİN DENETİM SORULARI

Bu belgeyi inceleyecek olan harici modeller ve uzmanların yanıtlaması rica edilen teknik sorular:

1. **Mikro-Sermaye Dinamiği:** $10-$50 sermaye aralığında Binance Futures $5.00 minimum nosyonel (min notional) kuralı düşünüldüğünde, pozisyon boyutlandırma formülümüzde ($0.25 max risk / stop_dist) kaldıraç marjı sıkışması yaşanabilir mi?
2. **Poisson Lambda Bozunumu:** Dixon-Coles veya Hawkes süreçleri göz önüne alındığında, 5 dakikalık mum kapanışları arasındaki emir defteri mikroyapısında (OFI) asimetri eşiğimiz ($0.48 / 0.52$) daha dinamik bir eşik değeriyle (örneğin hareketli standart sapma ile) güncellenmeli midir?
3. **Erken Başabaş (BE Lock) Mesafesi:** $+0.28\%$ kârda devreye giren ve $+0.13\%$ net seviyeye çekilen Erken Başabaş kilidimiz, yüksek volatiliteli altcoinlerde (örn. PEPE, SUI) "erken kâr realizasyonu" nedeniyle büyük trend kazançlarını (fat-tail trends) kırpma eğiliminde midir? Gevşetilmeli midir yoksa sermaye koruma için bu katılıkta kalması optimum mudur?
4. **Confluence Sniper Güvenliği:** 5m SMA(50) ve Pivot Direncin $\%0.25$ bandında çakıştığı durumlarda taban kırılımını beklemeden en tepeden girilen bu kurgu, ani bir kurumsal likidite vakumunda (Short Squeeze) ek bir koruma gerektirir mi?

---
*Bu teknik şartname, AWS EC2 üzerinde aktif çalışan `binance_futures_bot` kaynak kodları ve bağımsız denetim loglarıyla %100 doğrulanmıştır.*
