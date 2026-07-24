#!/usr/bin/env python3
"""Testes da amostragem do ClickBench (`run_m128_clickbench._ensure_sample`).

O bug que estes testes travam: o subsample usava `head -n`, que devolve as PRIMEIRAS n linhas. Como o
`hits` está ordenado por EventTime, isso é uma fatia temporal estreita — as cardinalidades de GROUP BY
ficam artificialmente baixas, e é exatamente o regime em que o pushdown vetorizado mais brilha. Ou seja:
a amostragem enviesava o benchmark **a nosso favor**.

Rodar: `python3 benchmarks/tests/test_clickbench_sample.py` (o repo não linka pytest em CI para os
benchmarks; este arquivo É o teste — sai != 0 em qualquer violação).
"""
import os
import sys
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

import run_m128_clickbench as cb  # noqa: E402


class CapturedCmd:
    """Captura o comando shell que `_ensure_sample` monta, sem executá-lo."""

    def __init__(self):
        self.cmd = None

    def __call__(self, argv, **kw):
        # argv == ["bash", "-c", cmd]
        self.cmd = argv[2]
        return mock.Mock(returncode=0)


class TestEnsureSample(unittest.TestCase):
    def setUp(self):
        self.cap = CapturedCmd()
        # o path não pode existir, senão a função retorna cedo (cache hit)
        self.path = "/tmp/__theodb_test_sample_does_not_exist.tsv"
        if os.path.exists(self.path):
            os.unlink(self.path)

    def _run(self, n, strategy):
        # `isfile` é consultado DUAS vezes: no early-return de cache (precisa ser False, senão a função
        # sai antes de montar o comando) e na verificação do resultado (True). O side_effect modela isso.
        with mock.patch.object(cb.subprocess, "run", self.cap), \
             mock.patch.object(cb.os.path, "isfile", side_effect=[False, True]), \
             mock.patch.object(cb.os.path, "getsize", return_value=1):
            cb._ensure_sample(self.path, n, strategy)
        self.assertIsNotNone(self.cap.cmd, "o comando de stream não chegou a ser montado")
        return self.cap.cmd

    def test_systematic_e_o_default(self):
        """Regressão do viés: quem não passa estratégia recebe a amostragem SISTEMÁTICA, não `head`."""
        import inspect
        sig = inspect.signature(cb._ensure_sample)
        self.assertEqual(sig.parameters["strategy"].default, "systematic",
                         "o default precisa ser 'systematic' — 'head' enviesa o benchmark a nosso favor")

    def test_systematic_usa_awk_e_nao_head_isolado(self):
        """A amostra sistemática varre o arquivo (awk NR % k), cobrindo todo o intervalo temporal."""
        cmd = self._run(1_000_000, "systematic")
        self.assertIn("awk", cmd, "amostragem sistemática precisa do awk para espalhar as linhas")
        self.assertIn("NR %", cmd)

    def test_k_derivado_do_total_publicado(self):
        """k = total/n. Para 1M de 99.997.497, k=99 — 1 linha a cada 99, cobrindo o arquivo inteiro."""
        cmd = self._run(1_000_000, "systematic")
        self.assertIn(f"NR % {cb.HITS_TOTAL_ROWS // 1_000_000} ==", cmd)

    def test_k_nunca_zero(self):
        """EDGE: n maior que o total (pedir 200M de um arquivo de 100M) não pode gerar `NR % 0` (div-by-zero no awk)."""
        cmd = self._run(200_000_000, "systematic")
        self.assertIn("NR % 1 ==", cmd, "k precisa ter piso 1 quando n >= total")

    def test_head_continua_disponivel_para_smoke(self):
        """`head` segue acessível para smoke-test rápido — mas só quando pedido explicitamente."""
        cmd = self._run(1_000, "head")
        self.assertIn("head -n 1000", cmd)
        self.assertNotIn("awk", cmd)

    def test_estrategia_desconhecida_falha_alto(self):
        """NEGATIVE: uma estratégia inválida levanta erro tipado — nunca cai silenciosamente num default."""
        with self.assertRaises(ValueError) as ctx:
            cb._ensure_sample(self.path, 1000, "aleatoria")
        self.assertIn("aleatoria", str(ctx.exception))

    def test_total_publicado_bate_com_o_clickbench(self):
        """O k depende deste número; se alguém o alterar sem fonte, a amostragem silenciosamente desalinha."""
        self.assertEqual(cb.HITS_TOTAL_ROWS, 99_997_497,
                         "total de linhas do ClickBench hits — número publicado pelo próprio ClickBench")


if __name__ == "__main__":
    unittest.main(verbosity=2)
