"""Testes da extensão Kokoro: python3 -m unittest discover -s extensions/kokoro

Só a parte que não precisa do modelo (configuração e caminhos): gerar voz de
verdade baixaria ~120 MB.
"""

import importlib.machinery
import importlib.util
import os
import unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
loader = importlib.machinery.SourceFileLoader("term7_kokoro", os.path.join(HERE, "term7-kokoro"))
spec = importlib.util.spec_from_loader("term7_kokoro", loader)
ext = importlib.util.module_from_spec(spec)
loader.exec_module(ext)


class Configuracao(unittest.TestCase):
    def test_le_o_arquivo_que_o_term7_grava(self):
        # Formato do render_env do extensions.rs.
        text = 'KOKORO_LANG="pt-br"\nKOKORO_SPEED="0.9"\nKOKORO_VOICE="pf_dora"\n# comentário\n'
        self.assertEqual(ext.parse_env(text), {"KOKORO_LANG": "pt-br", "KOKORO_SPEED": "0.9", "KOKORO_VOICE": "pf_dora"})
        self.assertEqual(ext.parse_env('X="com \\"aspas\\""'), {"X": 'com "aspas"'})

    def test_padroes_e_validacao(self):
        cfg = ext.settings({})
        self.assertEqual((cfg["voice"], cfg["lang"], cfg["speed"]), ("pm_alex", "pt-br", 1.0))
        # Vírgula (teclado pt-BR) e limites.
        self.assertEqual(ext.settings({"KOKORO_SPEED": "0,8"})["speed"], 0.8)
        self.assertEqual(ext.settings({"KOKORO_SPEED": "9"})["speed"], 2.0)
        self.assertEqual(ext.settings({"KOKORO_SPEED": "rápido"})["speed"], 1.0)
        self.assertEqual(ext.settings({"KOKORO_THREADS": "0"})["threads"], 1)
        self.assertEqual(ext.settings({"KOKORO_THREADS": "x"})["threads"], 4)
        # Vazio não apaga o padrão.
        self.assertEqual(ext.settings({"KOKORO_VOICE": ""})["voice"], "pm_alex")

    def test_pastas_seguem_o_xdg(self):
        with mock.patch.dict(os.environ, {"XDG_DATA_HOME": "/d", "XDG_CONFIG_HOME": "/c"}):
            self.assertEqual(ext.data_dir(), "/d/term7/kokoro")
            self.assertEqual(ext.env_file(), "/c/term7/kokoro.env")
            self.assertEqual(ext.venv_python(), "/d/term7/kokoro/venv/bin/python")


class Uso(unittest.TestCase):
    def test_sem_instalar_avisa(self):
        with mock.patch.dict(os.environ, {"XDG_DATA_HOME": "/nao/existe"}):
            with self.assertRaises(SystemExit) as e:
                ext.main(["term7-kokoro", "/tmp/x.wav"])
        self.assertIn("não terminou de instalar", str(e.exception.code))

    def test_argumentos_errados(self):
        with self.assertRaises(SystemExit):
            ext.main(["term7-kokoro"])
        with self.assertRaises(SystemExit):
            ext.main(["term7-kokoro", "--outra"])


if __name__ == "__main__":
    unittest.main()
