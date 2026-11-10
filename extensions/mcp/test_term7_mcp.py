"""Testes do MCP do term7: python3 -m unittest discover -s extensions/mcp"""

import importlib.machinery
import importlib.util
import json
import os
import socket
import tempfile
import threading
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
loader = importlib.machinery.SourceFileLoader("term7_mcp", os.path.join(HERE, "term7-mcp"))
spec = importlib.util.spec_from_loader("term7_mcp", loader)
mcp = importlib.util.module_from_spec(spec)
loader.exec_module(mcp)

TABS = [
    {"id": 1, "title": "term7", "cwd": "/r/term7", "branch": "feat/x", "process": "claude", "claude": "waiting",
     "attention": "waiting", "tag": None, "paused": False, "active": True, "self": False, "queued": 0},
    {"id": 2, "title": "gopay", "cwd": "/r/gopay", "branch": None, "process": "zsh", "claude": None,
     "attention": None, "tag": "review", "paused": True, "active": False, "self": True, "queued": 0},
]


class Protocolo(unittest.TestCase):
    def test_initialize_usa_a_versao_pedida_se_conhecer(self):
        r = mcp.respond({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26"}})
        self.assertEqual(r["result"]["protocolVersion"], "2025-03-26")
        r = mcp.respond({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "1999"}})
        self.assertEqual(r["result"]["protocolVersion"], mcp.PROTOCOLS[0])

    def test_notificacao_nao_tem_resposta(self):
        self.assertIsNone(mcp.respond({"jsonrpc": "2.0", "method": "notifications/initialized"}))

    def test_lista_as_ferramentas(self):
        r = mcp.respond({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        self.assertEqual([t["name"] for t in r["result"]["tools"]], ["list_tabs", "read_tab", "send_to_tab"])

    def test_erro_da_api_vira_resultado_com_isError(self):
        def api(method, params=None):
            raise mcp.ApiError("aba 9 não existe")

        r = mcp.respond({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                         "params": {"name": "read_tab", "arguments": {"tab": 9}}}, api)
        self.assertTrue(r["result"]["isError"])
        self.assertIn("aba 9", r["result"]["content"][0]["text"])

    def test_metodo_desconhecido(self):
        self.assertEqual(mcp.respond({"jsonrpc": "2.0", "id": 4, "method": "x"})["error"]["code"], -32601)


class Ferramentas(unittest.TestCase):
    def test_lista_destaca_quem_espera(self):
        text = mcp.run_tool("list_tabs", {}, lambda m, p=None: TABS)
        self.assertIn("[1] term7 (em foco, não vista)", text)
        self.assertIn("ESPERANDO VOCÊ", text)
        self.assertIn("[2] gopay (esta aba, pausada)", text)
        self.assertIn("programa: zsh (sem Claude)", text)

    def test_envio_na_fila(self):
        text = mcp.run_tool("send_to_tab", {"tab": 1, "message": "oi"},
                            lambda m, p=None: {"status": "queued", "position": 2})
        self.assertIn("posição 2", text)


class Socket(unittest.TestCase):
    def test_fala_com_a_api_e_repassa_o_pane(self):
        path = os.path.join(tempfile.mkdtemp(), "api.sock")
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(path)
        server.listen(1)
        got = {}

        def serve():
            conn, _ = server.accept()
            with conn:
                got["req"] = json.loads(conn.makefile().readline())
                conn.sendall(b'{"result": {"status": "sent"}}\n')

        t = threading.Thread(target=serve)
        t.start()
        os.environ["TERM7_PANE"] = "123-4"
        try:
            res = mcp.call_api("tabs.send", {"tab": 1, "text": "oi"}, path=path)
        finally:
            del os.environ["TERM7_PANE"]
            t.join()
            server.close()
        self.assertEqual(res, {"status": "sent"})
        self.assertEqual(got["req"], {"method": "tabs.send", "params": {"tab": 1, "text": "oi", "pane": "123-4"}})

    def test_term7_fechado(self):
        with self.assertRaises(mcp.ApiError) as e:
            mcp.call_api("tabs.list", path="/nao/existe.sock")
        self.assertIn("não está aberto", str(e.exception))


if __name__ == "__main__":
    unittest.main()
