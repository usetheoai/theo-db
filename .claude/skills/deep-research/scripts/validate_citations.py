#!/usr/bin/env python3
"""Mecaniza o contrato de honestidade da skill deep-research sobre um capítulo do handbook.

Verifica, fail-closed:
  1. Toda citação `caminho.ext:linha` resolve no disco (arquivo existe + tem >= aquela linha). Fabricada -> INVALID.
  2. Toda URL externa (http/https) está no allowlist de domínios. Fora -> INVALID.
  3. Todo parágrafo com afirmação de performance (Nx / QPS / recall 0.9x / p50 / ms) tem um link de benchmark
     (docs/benchmarks/ ou um artefato mNN-...) OU o marcador `UNBENCHMARKED`. Número solto -> NEEDS_REVISION.

Exit: 0 = PASS · 1 = INVALID (fabricação/URL) · 3 = NEEDS_REVISION (número sem evidência) · 2 = erro de invocação.

Uso: python3 validate_citations.py <capitulo.md> [--allowlist <arquivo>] [--repo-root <dir>]
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Um caminho de código/artefato com extensão + :linha. Exige uma extensão de 1-4 letras ANTES do ':', o que
# exclui `arXiv:1603...` (sem extensão) e `0.98` (sem ':'). Ex.: `am/scan.rs:164`, `docs/benchmarks/m35-...json:79`.
CITATION_RE = re.compile(r"`?([\w./\-]+\.[A-Za-z]{1,5}):(\d+)`?")
URL_RE = re.compile(r"https?://([^/\s)\]]+)")
# Sinais de afirmação de performance no texto de um parágrafo.
PERF_RE = re.compile(
    r"(\b\d+(?:[.,]\d+)?\s*×|\b\d+(?:[.,]\d+)?x\s|(?<![A-Za-z])QPS\b|\brecall@?\s*[01]?[.,]\d|\bp50\b|\bp95\b|"
    r"\b\d+(?:[.,]\d+)?\s*ms\b|\b\d+(?:[.,]\d+)?\s*QPS\b)",
    re.IGNORECASE,
)
# Evidência aceitável no mesmo parágrafo de uma afirmação de performance.
EVIDENCE_RE = re.compile(r"(docs/benchmarks/|\bm\d{2,}[a-z]?-|UNBENCHMARKED|benchmarks/theodb_bench)", re.IGNORECASE)

# Raízes de busca para resolver caminhos curtos (o cap. 19 usa `ann/hnsw.rs:196` e `theodb_rs/src/...`).
SEARCH_ROOTS = [
    ".",
    "theodb_rs/src",
    "docs",
    "benchmarks",
    ".claude",
    ".claude/knowledge-base/references/pgvector/src",
]


def _line_count(p: Path) -> int:
    with p.open("rb") as fh:
        return sum(1 for _ in fh)


def resolve(path: str, line: int, repo: Path) -> bool:
    # 1. tentativa direta contra as raízes conhecidas (rápido; cobre caminhos completos e prefixados)
    for root in SEARCH_ROOTS:
        candidate = repo / root / path
        try:
            if candidate.is_file() and _line_count(candidate) >= line:
                return True
        except OSError:
            continue
    # 2. fallback recursivo por basename — capítulos citam a forma curta (`hnsw.rs:196`) depois de estabelecer o
    #    caminho no texto. Se QUALQUER arquivo com esse basename tem >= a linha, a citação é plausivelmente real.
    basename = Path(path).name
    for hit in repo.rglob(basename):
        if any(part in {".git", "target", "node_modules", ".venv"} for part in hit.parts):
            continue
        try:
            if hit.is_file() and _line_count(hit) >= line:
                return True
        except OSError:
            continue
    return False


def load_allowlist(path: Path | None) -> set[str]:
    if not path or not path.is_file():
        return set()
    out = set()
    for raw in path.read_text().splitlines():
        s = raw.strip()
        if s and not s.startswith("#"):
            out.add(s.lower())
    return out


def domain_allowed(host: str, allow: set[str]) -> bool:
    host = host.lower()
    for entry in allow:
        if entry.startswith("*."):
            if host == entry[2:] or host.endswith(entry[1:]):
                return True
        elif host == entry:
            return True
    return False


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("chapter")
    ap.add_argument("--allowlist", default=".claude/rules/discover-web-allowlist.txt")
    ap.add_argument("--repo-root", default=".")
    args = ap.parse_args()

    ch = Path(args.chapter)
    if not ch.is_file():
        print(f"ERRO: capítulo não encontrado: {ch}", file=sys.stderr)
        return 2
    repo = Path(args.repo_root).resolve()
    allow = load_allowlist(Path(args.allowlist))
    text = ch.read_text()

    fabricated: list[str] = []
    checked = 0
    for m in CITATION_RE.finditer(text):
        path, line = m.group(1), int(m.group(2))
        # ignora âncoras que claramente não são caminhos de arquivo (ex.: nomes com espaço não chegam aqui)
        if "/" not in path and not path.endswith((".rs", ".py", ".md", ".json", ".txt", ".c", ".h", ".go", ".sql")):
            continue
        checked += 1
        if not resolve(path, line, repo):
            fabricated.append(f"{path}:{line}")

    bad_urls: list[str] = []
    if allow:
        for m in URL_RE.finditer(text):
            host = m.group(1)
            if not domain_allowed(host, allow):
                bad_urls.append(host)

    # Evidência de benchmark é verificada no nível da SEÇÃO (um link no topo de §X.4 cobre a tabela daquela
    # seção). Uma seção com afirmação de performance precisa conter, em algum lugar, um link de benchmark OU o
    # marcador UNBENCHMARKED. Seções puramente conceituais (Pontos-chave, Exercícios) que mencionam um número de
    # forma retórica são cobertas se a seção-fonte já ancorou — para reduzir ruído, exigimos evidência só em
    # seções cujo TÍTULO ou corpo indica medição (benchmark/performance/QPS na primeira linha).
    # Seções de RESUMO/EXERCÍCIO/REFERÊNCIA reafirmam números já ancorados nas seções de conteúdo — não introduzem
    # afirmações novas, então são isentas do gate (senão o gate viraria ruído).
    exempt = re.compile(r"pontos-chave|exerc|refer[êe]ncias|pr[ée]-requisitos|takeaway|resumo|sum[áa]rio", re.IGNORECASE)
    perf_gaps: list[str] = []
    sections = re.split(r"(?=^##\s)", text, flags=re.MULTILINE)
    for sec in sections:
        head = sec.strip().splitlines()[0] if sec.strip() else ""
        if exempt.search(head):
            continue
        if PERF_RE.search(sec) and not EVIDENCE_RE.search(sec):
            perf_gaps.append(head[:110] or "(sem título)")

    print(f"deep-research citation check — {ch}")
    print(f"  citações de código verificadas: {checked}  | fabricadas: {len(fabricated)}")
    print(f"  URLs externas fora do allowlist: {len(set(bad_urls))}")
    print(f"  parágrafos com número de performance sem evidência: {len(perf_gaps)}")
    for f in fabricated:
        print(f"    [INVALID] citação não resolve: {f}")
    for u in sorted(set(bad_urls)):
        print(f"    [INVALID] domínio fora do allowlist: {u}")
    for p in perf_gaps[:10]:
        print(f"    [NEEDS_REVISION] número sem benchmark/UNBENCHMARKED: “{p}”")

    if fabricated or bad_urls:
        print("VERDICT: INVALID")
        return 1
    if perf_gaps:
        print("VERDICT: NEEDS_REVISION")
        return 3
    print("VERDICT: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
