"""Disk-cached OpenAI embedder for the real-BEIR hybrid eval (M53).

Two-phase by design so the DB run stays deterministic and offline:

  1. `warm(texts)` — pre-embeds every corpus + query text ONCE via the OpenAI embeddings endpoint
     (batched, ≤ MAX_BATCH inputs/request) and persists each vector to disk keyed by sha256(text).
  2. `as_embed_fn()` — returns a pure closure `f(text) -> list[float]` that only reads the warm cache.
     No network, deterministic, so re-running the retriever N times yields byte-identical rankings.

Rule 9: stdlib only (urllib / json / hashlib / os) — no `openai`/`requests` dependency. The endpoint
is hit with a hand-built POST; the API key is read from `$OPENAI_API_KEY` at request time and NEVER
logged, echoed, or persisted. Fail-fast + typed (Rule 8): a missing key raises RuntimeError, a
dimension mismatch or an all-zero vector raises ValueError, and an un-warmed lookup raises KeyError —
no silent zero-vector fallbacks that would quietly corrupt recall.
"""
from __future__ import annotations

import hashlib
import json
import os
import urllib.error
import urllib.request
from pathlib import Path

OPENAI_EMBEDDINGS_URL = "https://api.openai.com/v1/embeddings"
# The endpoint caps a request BOTH by input count (2048) AND by total enqueued tokens (~300k). A large
# BEIR batch blows the token cap long before the input cap, so we batch conservatively by input count to
# stay well under the token budget (128 × ~500 tok ≈ 64k ≪ 300k).
MAX_BATCH = 128
# Per-input hard cap of text-embedding-3-* is 8191 tokens; truncate defensively by chars (~4 chars/token)
# so one pathological long doc cannot 400 the whole batch. BEIR abstracts are far shorter; this is a guard.
_MAX_INPUT_CHARS = 28000


class CachedOpenAIEmbedder:
    def __init__(self, model: str = "text-embedding-3-small", dim: int = 1536,
                 cache_dir: str = "benchmarks/.cache"):
        self.model = model
        self.dim = dim
        self.cache_path = Path(cache_dir) / "embeddings" / f"{model}-{dim}.jsonl"
        self._cache: dict[str, list] = {}
        self._load_cache()

    # --- disk cache (one JSON object per line: {"h": sha256, "v": [floats]}) -----------------------
    def _load_cache(self) -> None:
        if not self.cache_path.exists():
            return
        with self.cache_path.open(encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                obj = json.loads(line)
                self._cache[obj["h"]] = obj["v"]

    def _append_to_disk(self, h: str, vec: list) -> None:
        self.cache_path.parent.mkdir(parents=True, exist_ok=True)
        with self.cache_path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps({"h": h, "v": vec}) + "\n")

    @staticmethod
    def _hash(text: str) -> str:
        return hashlib.sha256(text.encode("utf-8")).hexdigest()

    @staticmethod
    def _sanitize(text: str) -> str:
        """The payload the API sees: the endpoint 400s on an empty string, so a whitespace-only text is
        sent as a single space; over-long text is truncated to the per-input token cap. The CACHE KEY is
        still the ORIGINAL text's hash, so `as_embed_fn(original)` resolves — only the wire payload changes.
        """
        s = text if text.strip() else " "
        return s[:_MAX_INPUT_CHARS]

    @staticmethod
    def _assert_no_zero_vectors(vec: list) -> None:
        """A degenerate all-zero embedding silently destroys cosine ranking — refuse to cache it."""
        if not any(x != 0.0 for x in vec):
            raise ValueError("refusing to cache an all-zero embedding vector (degenerate)")

    # --- network boundary (the ONLY I/O; monkeypatched in unit tests) -----------------------------
    def _fetch_embeddings(self, texts: list) -> list:
        """POST `texts` to the OpenAI embeddings endpoint; return vectors in the SAME order.

        Fails loud if `$OPENAI_API_KEY` is absent (checked BEFORE any network call). The key is only
        ever placed in the Authorization header — never logged or persisted.
        """
        api_key = os.environ.get("OPENAI_API_KEY")
        if not api_key:
            raise RuntimeError(
                "OPENAI_API_KEY not set — cannot fetch embeddings. "
                "Pre-warm with the key present; the DB run itself uses only the offline cache."
            )
        inputs = [self._sanitize(t) for t in texts]
        payload = json.dumps({"model": self.model, "input": inputs, "dimensions": self.dim}).encode("utf-8")
        req = urllib.request.Request(
            OPENAI_EMBEDDINGS_URL,
            data=payload,
            headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
            method="POST",
        )
        # Retry on transient rate-limit (429) / server (5xx) errors with exponential backoff; a 4xx other than
        # 429 is a hard error (bad request/auth) and is not retried. The whole-corpus warm can brush the TPM
        # ceiling, so a bounded backoff lets a large eval complete instead of aborting on a 16ms rate window.
        import time as _time
        last_detail = ""
        for attempt in range(6):
            try:
                with urllib.request.urlopen(req, timeout=120) as resp:  # noqa: S310 — fixed https OpenAI host
                    body = json.loads(resp.read().decode("utf-8"))
                break
            except urllib.error.HTTPError as e:
                # Surface the API's reason (never the key) so a 400 is diagnosable instead of opaque.
                last_detail = e.read().decode("utf-8", "replace")[:500] if e.fp else ""
                if e.code == 429 or 500 <= e.code < 600:
                    if attempt < 5:
                        _time.sleep(min(2.0 ** attempt, 30.0))
                        continue
                raise RuntimeError(f"OpenAI embeddings HTTP {e.code}: {last_detail}") from None
        else:
            raise RuntimeError(f"OpenAI embeddings: exhausted retries; last={last_detail}")
        # OpenAI returns objects carrying their input `index`; sort to guarantee input alignment.
        items = sorted(body["data"], key=lambda d: d["index"])
        return [it["embedding"] for it in items]

    # --- public API ------------------------------------------------------------------------------
    def warm(self, texts: list) -> None:
        """Ensure every text in `texts` has a cached embedding. Only un-cached texts hit the API."""
        to_fetch: list = []
        seen: set = set()
        for text in texts:
            h = self._hash(text)
            if h in self._cache or h in seen:
                continue
            seen.add(h)
            to_fetch.append(text)

        for start in range(0, len(to_fetch), MAX_BATCH):
            batch = to_fetch[start:start + MAX_BATCH]
            vectors = self._fetch_embeddings(batch)
            if len(vectors) != len(batch):
                raise ValueError(
                    f"embedding count mismatch: sent {len(batch)} texts, got {len(vectors)} vectors"
                )
            for text, vec in zip(batch, vectors):
                if len(vec) != self.dim:
                    raise ValueError(f"embedding dim mismatch: expected {self.dim}, got {len(vec)}")
                self._assert_no_zero_vectors(vec)
                h = self._hash(text)
                self._cache[h] = vec
                self._append_to_disk(h, vec)

    def as_embed_fn(self):
        """Return a pure, offline `f(text) -> list[float]` reading only the warm cache (deterministic).

        Raises KeyError if `text` was not pre-warmed — no silent zero-vector fallback (Rule 8).
        """
        cache = self._cache
        hash_fn = self._hash

        def embed(text: str) -> list:
            h = hash_fn(text)
            try:
                return cache[h]
            except KeyError:
                raise KeyError(
                    f"text not pre-warmed (call warm() first): {text[:60]!r}"
                ) from None

        return embed
