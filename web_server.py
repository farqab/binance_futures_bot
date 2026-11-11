import http.server
import socketserver
import json
import os
import urllib.parse

PORT = 8080
DIRECTORY = os.path.dirname(os.path.abspath(__file__))
CONTROL_FILE = os.path.join(DIRECTORY, "bot_control.json")

def get_trading_enabled():
    if os.path.exists(CONTROL_FILE):
        try:
            with open(CONTROL_FILE, "r", encoding="utf-8") as f:
                data = json.load(f)
                return bool(data.get("trading_enabled", True))
        except Exception:
            pass
    return True

def set_trading_enabled(enabled):
    try:
        tmp_file = CONTROL_FILE + ".tmp"
        with open(tmp_file, "w", encoding="utf-8") as f:
            json.dump({"trading_enabled": bool(enabled)}, f)
        os.replace(tmp_file, CONTROL_FILE)
        return True
    except Exception as e:
        print(f"Error saving control file: {e}")
        return False

class BotHTTPHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=DIRECTORY, **kwargs)

    def end_headers(self):
        # Prevent caching for live data
        if any(self.path.endswith(ext) for ext in [".json", ".js", ".html"]) or "/api/" in self.path:
            self.send_header("Cache-Control", "no-cache, no-store, must-revalidate")
            self.send_header("Pragma", "no-cache")
            self.send_header("Expires", "0")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        super().end_headers()

    def do_OPTIONS(self):
        self.send_response(200)
        self.end_headers()

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path == "/api/toggle_trading":
            current = get_trading_enabled()
            new_state = not current
            set_trading_enabled(new_state)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"success": True, "trading_enabled": new_state}).encode("utf-8"))
            return

        elif path == "/api/trading_status":
            enabled = get_trading_enabled()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"trading_enabled": enabled}).encode("utf-8"))
            return

        elif path == "/api/set_trading":
            params = urllib.parse.parse_qs(parsed.query)
            val = params.get("enabled", ["true"])[0].lower()
            new_state = (val in ["true", "1", "yes"])
            set_trading_enabled(new_state)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"success": True, "trading_enabled": new_state}).encode("utf-8"))
            return

        super().do_GET()

    def do_POST(self):
        if self.path.startswith("/api/toggle_trading"):
            return self.do_GET()
        super().do_GET()

if __name__ == "__main__":
    os.chdir(DIRECTORY)
    # Enable address reuse so restarts don't fail with Address already in use
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("", PORT), BotHTTPHandler) as httpd:
        print(f"🚀 Quant Bot Web Server running on http://0.0.0.0:{PORT} (Serving {DIRECTORY})")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            pass
