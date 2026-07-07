"""BEIR-style labelled dataset (corpus / queries / qrels) + a deterministic lexical embedder.

For CI the eval uses a small, hand-labelled SYNTHETIC corpus so the 3-retriever run is fully
deterministic and offline (no embedding-endpoint dependency — plan ADR D4). `lexical_embed` derives a
fixed-dim vector from token counts (feature hashing), so the vector leg is correlated with content
without a model. The decision-grade real-BEIR slice (with theodb.embed over a real endpoint) is run
out-of-CI for the benchmark report.
"""
from __future__ import annotations

import json
import re
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

EMBED_DIM = 16
_TOKEN_RE = re.compile(r"[a-z0-9]+")

# The canonical public BEIR dataset host (Thakur et al. 2021). Each dataset is a zip that extracts to
# `{name}/corpus.jsonl`, `{name}/queries.jsonl`, `{name}/qrels/{split}.tsv`.
BEIR_BASE_URL = "https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets"


@dataclass(frozen=True)
class Dataset:
    corpus: dict      # doc_id -> content text
    queries: dict     # query_id -> query text
    qrels: dict       # query_id -> {doc_id: relevance_grade}


def lexical_embed(text: str, dim: int = EMBED_DIM) -> list:
    """Deterministic feature-hashed, L2-normalized count vector. No model, fully reproducible."""
    vec = [0.0] * dim
    for tok in _TOKEN_RE.findall(text.lower()):
        vec[hash_token(tok) % dim] += 1.0
    norm = sum(x * x for x in vec) ** 0.5
    if norm == 0.0:
        return vec
    return [x / norm for x in vec]


def hash_token(tok: str) -> int:
    """Stable non-cryptographic hash (FNV-1a) — independent of PYTHONHASHSEED for reproducibility."""
    h = 2166136261
    for ch in tok.encode("utf-8"):
        h = ((h ^ ch) * 16777619) & 0xFFFFFFFF
    return h


def synthetic_dataset() -> Dataset:
    """A tiny hand-labelled corpus: two topics (databases, cooking) with graded qrels."""
    corpus = {
        "d1": "postgresql database management system with sql",
        "d2": "database indexing and query tuning for performance",
        "d3": "relational database transactions and concurrency",
        "d4": "vector database similarity search with embeddings",
        "d5": "distributed database replication and high availability",
        "d6": "baking sourdough bread at home",
        "d7": "italian pasta recipes with tomato sauce",
        "d8": "grilling vegetables on a barbecue",
        "d9": "brewing coffee with a french press",
        "d10": "chocolate cake recipe for beginners",
        "d11": "machine learning embeddings for semantic search",
        "d12": "full text search ranking with bm25 and tf-idf",
    }
    queries = {
        "q1": "database query performance tuning",
        "q2": "vector similarity search embeddings",
        "q3": "cooking recipes at home",
        "q4": "full text search ranking",
    }
    qrels = {
        "q1": {"d1": 2, "d2": 3, "d3": 2, "d5": 1},
        "q2": {"d4": 3, "d11": 2},
        "q3": {"d6": 2, "d7": 3, "d8": 2, "d10": 2},
        "q4": {"d12": 3, "d11": 1},
    }
    return Dataset(corpus=corpus, queries=queries, qrels=qrels)


def _download_beir(name: str, cache_dir: str) -> Path:
    """Download + extract the BEIR `{name}.zip` under `{cache_dir}/beir/`. Returns the dataset dir.

    Idempotent at the caller level: `load_beir_dataset` only calls this when `corpus.jsonl` is absent.
    """
    beir_root = Path(cache_dir) / "beir"
    beir_root.mkdir(parents=True, exist_ok=True)
    zip_path = beir_root / f"{name}.zip"
    url = f"{BEIR_BASE_URL}/{name}.zip"
    urllib.request.urlretrieve(url, zip_path)  # noqa: S310 — trusted BEIR host, https only
    with zipfile.ZipFile(zip_path) as zf:
        zf.extractall(beir_root)
    zip_path.unlink(missing_ok=True)
    return beir_root / name


def load_beir_dataset(name: str, cache_dir: str, split: str = "test",
                      limit_queries: int | None = None) -> Dataset:
    """Load a REAL BEIR labelled dataset (corpus / queries / qrels) into the harness `Dataset` shape.

    Downloads `{BEIR_BASE_URL}/{name}.zip` (stdlib only — urllib + zipfile) and caches it at
    `{cache_dir}/beir/{name}/`. If `corpus.jsonl` already exists there, the download is skipped
    (offline re-run). All dataset statistics (n_docs, n_queries, graded relevance) come from the
    downloaded manifest — nothing here is hard-coded, so the numbers are whatever the real BEIR ships.

    Retrieval text per doc is `title + " " + text` (stripped) — BEIR's standard concatenation.
    `queries` is filtered to only those present in the split's qrels (BEIR ships extra unlabelled
    queries). `qrels` maps `{qid: {doc_id: int(score)}}`.

    `limit_queries` (smoke only): keep the first N queries by sorted qid (stable) and restrict qrels to
    them. The corpus is NEVER subsampled — recall@100 over a subsampled corpus would be a different,
    dishonest metric.

    Fails loud (FileNotFoundError) if any of corpus/queries/qrels is missing after extraction.
    """
    dataset_dir = Path(cache_dir) / "beir" / name
    corpus_path = dataset_dir / "corpus.jsonl"
    queries_path = dataset_dir / "queries.jsonl"
    qrels_path = dataset_dir / "qrels" / f"{split}.tsv"

    if not corpus_path.exists():
        _download_beir(name, cache_dir)

    for required in (corpus_path, queries_path, qrels_path):
        if not required.exists():
            raise FileNotFoundError(
                f"BEIR dataset {name!r} incomplete after extraction: missing {required}"
            )

    # qrels first — it defines which queries are labelled (and thus retained).
    qrels: dict = {}
    with qrels_path.open(encoding="utf-8") as fh:
        header = fh.readline()  # `query-id\tcorpus-id\tscore` — skipped by design
        if not header:
            raise FileNotFoundError(f"BEIR qrels file is empty: {qrels_path}")
        for line in fh:
            line = line.rstrip("\n")
            if not line:
                continue
            qid, doc_id, score = line.split("\t")
            qrels.setdefault(qid, {})[doc_id] = int(score)

    corpus: dict = {}
    with corpus_path.open(encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            title = obj.get("title", "") or ""
            text = obj.get("text", "") or ""
            corpus[obj["_id"]] = (title + " " + text).strip()

    queries: dict = {}
    with queries_path.open(encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            qid = obj["_id"]
            if qid in qrels:  # keep only labelled queries (BEIR ships extras)
                queries[qid] = obj["text"]

    if limit_queries is not None:
        keep = sorted(queries)[:limit_queries]
        queries = {qid: queries[qid] for qid in keep}
        qrels = {qid: qrels[qid] for qid in keep}

    return Dataset(corpus=corpus, queries=queries, qrels=qrels)
