"""M168 unit tests — os guards dos DOIS summarizers, com controle positivo em árvore.

WHY. O `_reject_fallback` do A/B foi teatro em DUAS rodadas de review seguidas, e nas duas vezes a prosa do
comentário afirmava que ele funcionava:

  * 1ª versão: casava `theodb_topk_stream_fallback`, que `pgrx::log!` escreve no log do SERVIDOR — mas o coletor
    captura só o stdout do `psql`. O marcador nunca aparecia no artefato.
  * 2ª versão: casava `ARM=stream`, string que o `m168_stream_ab.sql` não emitia (a GUC alterna dentro do
    PL/pgSQL). Um ramo inalcançável trocado por outro.

Nas duas vezes o defeito só apareceu porque um revisor CONSTRUIU um controle positivo em vez de ler o código. É a
lição do M161 — medir alcançabilidade antes de entregar o guard — e ela não pode depender de alguém lembrar de
refazer o experimento à mão. Estes testes são esse controle, em árvore, e falham se o guard voltar a ser cego.

Terceiro caso, encontrado pelo mesmo método: um log SEM nenhum trace de streaming (canal `THEODB_ADMIT_TRACE`
desligado no postmaster) passava com "ok" e exit 0 — o guard não tinha como distinguir um braço íntegro de um
degradado, e dizia que estava tudo bem. Um guard cego que se declara satisfeito é pior que guard nenhum.

Lógica pura: cada teste escreve um log sintético mínimo em tmp_path e roda o summarizer como subprocesso. Sem
banco, sem caixa real, determinístico (mesmo desenho de `test_m164_harness_guards.py`).
"""
from __future__ import annotations

import os
import subprocess
import sys

BENCH = os.path.dirname(os.path.abspath(__file__))
AB = os.path.join(BENCH, "m168_ab_summarize.py")
PEAK = os.path.join(BENCH, "m168_peak_summarize.py")

# Um log de A/B mínimo, porém COMPLETO o bastante para o summarizer aceitar: cabeçalho de proveniência, o bloco de
# medianas com 4 consultas e n=2 pares, e o bloco per-pair. Os pares são pares (2) para não disparar o guard de
# contrabalanceamento. O trace de streaming está presente, então o guard de não-vacuidade fica satisfeito.
_AB_CLEAN = """so_md5=e010375381ae7ad9069e4a38a5d6c9c6
NOTICE:  ARM=eager pair=1 q=q23
theodb_decode_batch: rows=1000000 bytes=809738352
NOTICE:  ARM=stream pair=1 q=q23
theodb_decode_batch_stream: rows=10000 bytes=8097383
   q23   |  100.0 |   80.0 |  0.800 |  2
   q24   |  100.0 |  103.0 |  1.030 |  2
   q25   |  100.0 |   99.7 |  0.997 |  2
   q26   |  100.0 |   98.1 |  0.981 |  2
 per-pair detail
   q23   | 100.0 100.0 | 80.0 80.0
   q24   | 100.0 100.0 | 103.0 103.0
   q25   | 100.0 100.0 | 99.7 99.7
   q26   | 100.0 100.0 | 98.1 98.1
"""


def _run(script, log, *extra):
    return subprocess.run([sys.executable, script, str(log), *extra],
                          capture_output=True, text=True)


def _write(tmp_path, name, text):
    p = tmp_path / name
    p.write_text(text)
    return p


# ---------- o log íntegro passa (senão os testes de falha abaixo não provam nada) -------------------------------------
def test_ab_clean_log_passes():
    import tempfile, pathlib
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "clean.log", _AB_CLEAN)
        r = _run(AB, log, "--pairs", "2")
        assert r.returncode == 0, f"log íntegro deveria passar:\n{r.stdout}\n{r.stderr}"
        assert "ok:" in r.stdout


# ---------- CONTROLE POSITIVO 1: braço streaming que degradou para o eager --------------------------------------------
def test_ab_rejects_eager_decode_inside_stream_arm():
    """A assinatura REAL de uma degradação: um trace de decode eager depois de um marcador de braço streaming.
    Este é o sinal que as duas versões anteriores do guard não conseguiam ver."""
    import tempfile, pathlib
    poisoned = _AB_CLEAN.replace(
        "NOTICE:  ARM=stream pair=1 q=q23\n",
        "NOTICE:  ARM=stream pair=1 q=q23\ntheodb_decode_batch: rows=1000000 bytes=809738352\n")
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "degraded.log", poisoned)
        r = _run(AB, log, "--pairs", "2")
        assert r.returncode != 0, "um braço streaming que decodificou eager TEM de reprovar o log"
        assert "decode EAGER" in r.stdout


# ---------- CONTROLE POSITIVO 2: o marcador do fail-open, quando ele chega ao artefato ---------------------------------
def test_ab_rejects_explicit_fallback_marker():
    import tempfile, pathlib
    poisoned = _AB_CLEAN + "LOG:  theodb_topk_stream_fallback: Resources exhausted\n"
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "fallback.log", poisoned)
        r = _run(AB, log, "--pairs", "2")
        assert r.returncode != 0
        assert "theodb_topk_stream_fallback" in r.stdout


# ---------- CONTROLE POSITIVO 3: o guard CEGO (canal de trace desligado) ----------------------------------------------
def test_ab_rejects_log_without_any_stream_trace():
    """Sem `THEODB_ADMIT_TRACE=1` no postmaster não há trace algum, e o guard de degradação não consegue afirmar
    nada. Antes desta correção o summarizer imprimia 'ok' e saía 0 — publicando uma tabela cujo guard estava cego."""
    import tempfile, pathlib
    blind = "\n".join(l for l in _AB_CLEAN.splitlines()
                      if "theodb_decode_batch" not in l) + "\n"
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "blind.log", blind)
        r = _run(AB, log, "--pairs", "2")
        assert r.returncode != 0, "log sem trace de streaming deixa o guard cego — tem de reprovar"
        assert "CEGO" in r.stdout


# ---------- o guard de contrabalanceamento (pares ímpares não alternam) ------------------------------------------------
def test_ab_rejects_odd_pair_count():
    import tempfile, pathlib
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "clean.log", _AB_CLEAN)
        r = _run(AB, log, "--pairs", "3")
        assert r.returncode != 0
        assert "ÍMPAR" in r.stdout


# ---------- o summarizer de memória: um braço streaming com UM batch é comparação vazia --------------------------------
_PEAK_CLEAN = """so_md5=e010375381ae7ad9069e4a38a5d6c9c6
ARM=eager
===q23===
theodb_decode_batch: rows=1000000 bytes=809738352
ARM=stream
===q23===
theodb_decode_batch_stream: rows=10000 bytes=8097383
theodb_decode_batch_stream: rows=10000 bytes=8097383 probe=1
"""


def test_peak_clean_log_passes():
    import tempfile, pathlib
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "peak.log", _PEAK_CLEAN)
        r = _run(PEAK, log)
        assert r.returncode == 0, f"{r.stdout}\n{r.stderr}"


def test_peak_rejects_single_batch_stream_arm():
    """Um braço 'streaming' com um único batch não transmitiu nada: a razão eager/stream seria 1,0 por
    construção, e a tabela de 43× viraria ficção."""
    import tempfile, pathlib
    one_batch = _PEAK_CLEAN.replace(
        "theodb_decode_batch_stream: rows=10000 bytes=8097383\ntheodb_decode_batch_stream: rows=10000 bytes=8097383 probe=1\n",
        "theodb_decode_batch_stream: rows=1000000 bytes=809738352 probe=1\n")
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "onebatch.log", one_batch)
        r = _run(PEAK, log)
        assert r.returncode != 0
        assert "nada foi transmitido" in r.stdout


def test_peak_rejects_stream_arm_without_probe():
    """Sem a sonda (`probe=1`) o máximo é sobre N-1 chunk-groups, não N — o pico publicado seria um sub-máximo."""
    import tempfile, pathlib
    no_probe = _PEAK_CLEAN.replace(" probe=1", "")
    with tempfile.TemporaryDirectory() as d:
        log = _write(pathlib.Path(d), "noprobe.log", no_probe)
        r = _run(PEAK, log)
        assert r.returncode != 0
        assert "probe=1" in r.stdout
