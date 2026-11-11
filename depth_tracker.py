import json
import time
import os
import urllib.request

SYMBOLS = [
    "BTCUSDT", "ETHUSDT", "SOLUSDT", "DOGEUSDT", "XRPUSDT",
    "LINKUSDT", "SUIUSDT", "AVAXUSDT", "NEARUSDT", "ADAUSDT",
    "APTUSDT", "DOTUSDT", "TIAUSDT"
]

OUTPUT_FILE = "/home/ubuntu/binance_futures_bot/depth_walls.json"
TMP_FILE = OUTPUT_FILE + ".tmp"

def fetch_depth(symbol):
    url = f"https://fapi.binance.com/fapi/v1/depth?symbol={symbol}&limit=20"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(req, timeout=3) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            bids = [[float(p), float(q)] for p, q in data.get("bids", [])]
            asks = [[float(p), float(q)] for p, q in data.get("asks", [])]
            if not bids or not asks:
                return None
            max_bid = max(bids, key=lambda x: x[1])
            max_ask = max(asks, key=lambda x: x[1])
            top5_bid_sum = sum(x[1] for x in bids[:5])
            top5_ask_sum = sum(x[1] for x in asks[:5])
            ratio = top5_bid_sum / top5_ask_sum if top5_ask_sum > 0 else 1.0
            return {
                "buy_wall_price": max_bid[0],
                "buy_wall_qty": max_bid[1],
                "sell_wall_price": max_ask[0],
                "sell_wall_qty": max_ask[1],
                "imbalance_ratio": round(ratio, 2)
            }
    except Exception:
        return None

def main():
    while True:
        results = {}
        for sym in SYMBOLS:
            d = fetch_depth(sym)
            if d:
                results[sym] = d
            time.sleep(0.05) # respectful rate limit
        
        results["_updated_at"] = int(time.time() * 1000)
        try:
            with open(TMP_FILE, "w", encoding="utf-8") as f:
                json.dump(results, f)
            os.replace(TMP_FILE, OUTPUT_FILE)
        except Exception:
            pass
        time.sleep(4.0)

if __name__ == "__main__":
    main()
