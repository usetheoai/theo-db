"""Public log corpus loader for the M140.1 lexical-proxy axis (T0.2).

The theo-lens production trace corpus does not exist in the repo (exhaustive scout:
only demo fixtures of 2-6 spans). Owner decision 2026-07-22 (ADR D1): use a public log
corpus (LogHub — He et al., ISSRE 2020, arXiv:2008.06448) as a lexical trace-like proxy,
with the domain-fidelity caveat declared in the report; real-trace validation is the
boundary of M140.4/M141.

`_fixture` is a small embedded real-shaped log set for deterministic unit tests and the
`--smoke` runner path. Named real datasets (`hdfs`, `bgl`) are loaded from a cached file
under `cache_dir`; if absent, a clear FileNotFoundError with a download hint is raised
(never a silent empty corpus — error-handling.md fail-clear).
"""
from __future__ import annotations

import hashlib
import json
import random
from pathlib import Path

# Real-shaped log lines (HDFS/BGL/OTLP-trace flavour) — enough distinct tokens for a
# deterministic known-item unit test. NOT production traffic (ADR D1 caveat).
_FIXTURE: tuple[str, ...] = (
    "081109 203615 148 INFO dfs.DataNode$PacketResponder blk_38865049064139660 terminating",
    "081109 204005 35 INFO dfs.FSNamesystem BLOCK NameSystem.addStoredBlock blockMap updated 10.251.73.220 is added to blk_7128370237687728475 size 67108864",
    "081109 204132 26 WARN dfs.DataNode$DataXceiver writeBlock blk_5792489080791696128 received exception java.io.IOException Connection reset by peer",
    "R02-M1-N0-C:J12-U11 RAS KERNEL FATAL data storage interrupt caused by dcache parity error",
    "R21-M0-N4-I:J18-U01 RAS KERNEL INFO generating core.2131 for program mpirun rank 42 timeout waiting for barrier",
    "ERROR trace_id=abc123 span_id=def456 tool=Bash command timed out after 120000ms retry attempt 2 of 3",
    "INFO trace_id=abc124 span_id=def457 gen_ai.operation.name=chat model=claude-opus input_tokens=1842 output_tokens=903 latency_ms=2410",
    "WARN trace_id=abc125 span_id=def458 tool=Read file=/etc/passwd permission denied path outside workspace fail-closed 22023",
    "081109 205931 13 INFO dfs.DataNode$DataXceiver Receiving block blk_-1608999687919862906 src 10.250.19.102 dest 10.250.19.102",
    "081109 210012 42 INFO dfs.FSNamesystem BLOCK NameSystem.delete blk_-4491302241738371254 is added to invalidSet of 10.251.31.85",
    "R16-M1-N2-C:J05-U11 RAS APP INFO ciod suspending sync error reading message prefix on CioStream socket connection timed out",
    "ERROR trace_id=abc126 span_id=def459 embedding provider openai returned 429 rate limited backoff 8s",
    "INFO trace_id=abc127 span_id=def460 hybrid_search_rrf lexical=ts_rank_cd vector=cosine k=10 fused 47 candidates",
    "WARN dfs.DataNode$DataXceiver blk_904791815409399662 Slow BlockReceiver write packet to mirror took 1204ms",
    "081109 211455 17 INFO dfs.DataNode$PacketResponder Received block blk_2377150260128098806 of size 67108864 from 10.251.42.84",
    "R30-M0-N8-I:J20-U01 RAS KERNEL INFO instruction cache parity error corrected recovered without termination",
    "ERROR trace_id=abc128 span_id=def461 nl_to_sql rejected query indirect prompt injection detected in view parameter",
    "INFO trace_id=abc129 span_id=def462 vectorizer bgworker embedded 512 rows column content_embedding refreshed",
    "WARN trace_id=abc130 span_id=def463 columnar zonemap skip-pruned 8 of 10 stripes predicate ts > 2026-07-01 selective",
    "081109 212233 55 INFO dfs.FSNamesystem BLOCK NameSystem.allocateBlock /user/root/rand blk_8229193803249955061",
)


def _fixture_lines() -> list[str]:
    return list(_FIXTURE)


def _cached_dataset_lines(dataset: str, cache_dir: Path) -> list[str]:
    """Load a named LogHub dataset from a cached text file, else fail clear."""
    cache_file = cache_dir / f"{dataset}.log"
    if not cache_file.exists():
        raise FileNotFoundError(
            f"LogHub dataset '{dataset}' not cached at {cache_file}. "
            f"Download it (e.g. from https://github.com/logpai/loghub) and place the raw "
            f"log at {cache_file}, then re-run. No silent empty corpus."
        )
    return [ln for ln in cache_file.read_text(encoding="utf-8", errors="replace").splitlines() if ln.strip()]


def _source_lines(dataset: str, cache_dir: Path) -> list[str]:
    if dataset == "_fixture":
        return _fixture_lines()
    if dataset in ("hdfs", "bgl"):
        return _cached_dataset_lines(dataset, cache_dir)
    raise ValueError(f"unknown log dataset: {dataset!r} (known: _fixture, hdfs, bgl)")


def load_logcorpus(
    dataset: str = "hdfs",
    n: int = 2000,
    cache_dir: str = ".cache140",
    seed: int = 0,
) -> dict[str, str]:
    """Return `{doc_id: log_line}` — a deterministic sample of `n` lines from `dataset`.

    Determinism: same (dataset, n, seed) → same docs (random.Random(seed) over sorted
    indices). Caches the sampled result as jsonl under `cache_dir` for reproducibility.
    """
    cdir = Path(cache_dir)
    cdir.mkdir(parents=True, exist_ok=True)
    key = hashlib.sha1(f"{dataset}:{n}:{seed}".encode()).hexdigest()[:16]
    cache_path = cdir / f"logcorpus-{dataset}-{key}.jsonl"
    if cache_path.exists():
        return {
            d["id"]: d["text"]
            for d in (json.loads(line) for line in cache_path.read_text().splitlines() if line.strip())
        }

    lines = _source_lines(dataset, cdir)
    rng = random.Random(seed)
    count = min(n, len(lines))
    idx = sorted(rng.sample(range(len(lines)), count))
    docs = {f"log-{i}": lines[i].strip() for i in idx}

    with cache_path.open("w", encoding="utf-8") as fh:
        for doc_id, text in docs.items():
            fh.write(json.dumps({"id": doc_id, "text": text}) + "\n")
    return docs
