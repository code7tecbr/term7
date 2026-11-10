"""Testes da extensão statusbot: python3 -m unittest discover -s extensions/statusbot"""

import importlib.machinery
import importlib.util
import json
import os
import threading
import unittest
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
loader = importlib.machinery.SourceFileLoader("term7_statusbot", os.path.join(HERE, "term7-statusbot"))
spec = importlib.util.spec_from_loader("term7_statusbot", loader)
ext = importlib.util.module_from_spec(spec)
loader.exec_module(ext)

NOW = datetime(2026, 11, 10, 20, 10, tzinfo=timezone.utc)
# Resposta do statusbot no formato do README dele.
BODY = {
    "checkedAt": "2026-11-10T20:05:17.306Z",
    "intervalSeconds": 300,
    "services": 5,
    "signals": 2,
    "notUp": 1,
    "items": [
        {"label": "My API", "status": "down", "since": "2026-11-10T19:53:02.000Z", "detail": "HTTP 502"},
        {"label": "Cloudflare · Workers", "status": "up", "since": "2026-11-09T08:00:00.000Z"},
    ],
}


class ToStatus(unittest.TestCase):
    def test_leitura_normal(self):
        p = ext.to_status("http://x", 200, BODY, 60, NOW)
        self.assertEqual(p["source"], "statusbot")
        self.assertEqual(p["ttlSeconds"], 180)
        self.assertEqual(p["checkedAt"], BODY["checkedAt"])
        self.assertEqual(p["items"][0], BODY["items"][0])
        self.assertNotIn("error", p)

    def test_leitura_atrasada(self):
        late = datetime(2026, 11, 10, 21, 0, tzinfo=timezone.utc)
        self.assertIn("atrasada", ext.to_status("http://x", 200, BODY, 60, late)["error"])

    def test_erros(self):
        self.assertIn("primeira leitura", ext.to_status("http://x", 503, {"checkedAt": None}, 60)["error"])
        self.assertIn("token", ext.to_status("http://x", 401, None, 60)["error"])
        err = ext.to_status("http://x:1", 0, {"reason": "Connection refused"}, 60)["error"]
        self.assertIn("http://x:1/api/status", err)
        self.assertIn("Connection refused", err)

    def test_url(self):
        self.assertEqual(ext.status_url("http://x:8080/"), "http://x:8080/api/status")
        self.assertEqual(ext.status_url("https://s.com/api/status"), "https://s.com/api/status")


class Fetch(unittest.TestCase):
    def test_manda_o_token_e_le_o_json(self):
        seen = {}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                seen["auth"] = self.headers.get("Authorization")
                seen["path"] = self.path
                data = json.dumps(BODY).encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *a):
                pass

        server = HTTPServer(("127.0.0.1", 0), Handler)
        t = threading.Thread(target=server.handle_request)
        t.start()
        try:
            code, body = ext.fetch(f"http://127.0.0.1:{server.server_port}", "segredo")
        finally:
            t.join()
            server.server_close()
        self.assertEqual((code, body["notUp"]), (200, 1))
        self.assertEqual(seen, {"auth": "Bearer segredo", "path": "/api/status"})

    def test_sem_servidor(self):
        code, body = ext.fetch("http://127.0.0.1:9", timeout=2)
        self.assertEqual(code, 0)


class RunOnce(unittest.TestCase):
    def test_publica_no_term7(self):
        calls = []
        ext.fetch, original = (lambda url, token: (200, BODY)), ext.fetch
        try:
            ext.run_once("http://x", None, 60, lambda m, p: calls.append((m, p)))
        finally:
            ext.fetch = original
        self.assertEqual(calls[0][0], "status.set")
        self.assertEqual(len(calls[0][1]["items"]), 2)

    def test_term7_fechado(self):
        with self.assertRaises(ext.Term7Offline):
            ext.call_api("status.set", {}, path="/nao/existe.sock")


if __name__ == "__main__":
    unittest.main()
