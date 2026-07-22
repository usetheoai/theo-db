"""Known-item retrieval metrics + query generation (M140.1 T0.1).

Known-item retrieval (TREC named-page finding, Craswell & Hawking 2004): each query
targets ONE known document; the single relevant doc is that document. This is the
honest way to measure retrieval on a corpus WITHOUT human relevance judgments — the
gold label is the doc itself, nothing is fabricated (M140.1 ADR D1). Metrics: MRR@k
(Mean Reciprocal Rank), Success@1, Recall@k.

Reuses `metrics.recall_at_n` (Rule 9 / DRY) — recall of a singleton qrel.
"""
from __future__ import annotations

import re
from collections import Counter

from theodb_bench.metrics import recall_at_n

_TOKEN_RE = re.compile(r"[a-z0-9_]+")


def tokenize(text: str) -> list[str]:
    """Lowercase + split into alphanumeric/underscore tokens (keeps `blk_1234` whole)."""
    return _TOKEN_RE.findall(text.lower())


def mrr_at_k(ranked_ids: list[str], target_id: str, k: int) -> float:
    """Reciprocal rank of `target_id` in the top-k of `ranked_ids` (0.0 if absent).

    Fail-fast on k<=0 (error-handling.md): a non-positive window is a caller bug.
    """
    if k <= 0:
        raise ValueError(f"k must be > 0 (got {k})")
    for i, doc_id in enumerate(ranked_ids[:k]):
        if doc_id == target_id:
            return 1.0 / (i + 1)
    return 0.0


def success_at_1(ranked_ids: list[str], target_id: str) -> float:
    """1.0 iff the target doc is ranked first, else 0.0."""
    return 1.0 if ranked_ids[:1] == [target_id] else 0.0


def recall_known_item(ranked_ids: list[str], target_id: str, k: int) -> float:
    """Recall@k of the singleton relevant set {target_id} — reuses metrics.recall_at_n."""
    return recall_at_n(ranked_ids, {target_id: 1}, k)


def make_known_item_query(doc_text: str, rng, m: int = 5) -> str:
    """Build a query from the `m` most distinctive (rarest-in-doc) tokens of `doc_text`.

    Rarity = lowest local frequency, lexical tie-break — deterministic. `rng` shuffles
    the chosen terms so the query string is seed-deterministic yet varied across a
    corpus (testing.md §6: randomness is injected, never global). Empty doc → empty query.
    """
    toks = tokenize(doc_text)
    if not toks:
        return ""
    freq = Counter(toks)
    by_rarity = sorted(freq.keys(), key=lambda t: (freq[t], t))
    chosen = by_rarity[:m]
    rng.shuffle(chosen)
    return " ".join(chosen)
