"""BEIR-style labelled dataset (corpus / queries / qrels) + a deterministic lexical embedder.

For CI the eval uses a small, hand-labelled SYNTHETIC corpus so the 3-retriever run is fully
deterministic and offline (no embedding-endpoint dependency — plan ADR D4). `lexical_embed` derives a
fixed-dim vector from token counts (feature hashing), so the vector leg is correlated with content
without a model. The decision-grade real-BEIR slice (with theodb.embed over a real endpoint) is run
out-of-CI for the benchmark report.
"""
from __future__ import annotations

import re
from dataclasses import dataclass

EMBED_DIM = 16
_TOKEN_RE = re.compile(r"[a-z0-9]+")


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
