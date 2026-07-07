"""Pure-logic unit tests for the M53 real-BEIR harness (offline, deterministic — no DB, no network).

Covers the two pieces that carry decision-grade risk:
  * BEIR parsing — title+text join, query-by-qrels filtering, fail-loud on a missing file.
  * CachedOpenAIEmbedder — warm→lookup roundtrip, all-zero rejection, missing-key fail-loud. The network
    boundary (`_fetch_embeddings`) is monkeypatched so no request is ever made.
"""
import json

import pytest

from theodb_bench.beir import load_beir_dataset
from theodb_bench.openai_embed import CachedOpenAIEmbedder


# --- BEIR loader -----------------------------------------------------------------------------------
def _write_mini_beir(root, name):
    """Pre-populate `{root}/beir/{name}/` so load_beir_dataset never triggers a download."""
    d = root / "beir" / name
    (d / "qrels").mkdir(parents=True)
    corpus = [
        {"_id": "d1", "title": "Postgres", "text": "database system"},
        {"_id": "d2", "title": "Cooking", "text": "bread recipe"},
        {"_id": "d3", "title": "", "text": "no title doc"},
    ]
    (d / "corpus.jsonl").write_text("\n".join(json.dumps(x) for x in corpus) + "\n")
    queries = [
        {"_id": "q1", "text": "database"},
        {"_id": "q2", "text": "recipe"},
        {"_id": "q9", "text": "unlabelled query — not in qrels"},
    ]
    (d / "queries.jsonl").write_text("\n".join(json.dumps(x) for x in queries) + "\n")
    (d / "qrels" / "test.tsv").write_text(
        "query-id\tcorpus-id\tscore\n"
        "q1\td1\t1\n"
        "q2\td2\t2\n"
    )
    return d


def test_load_beir_parses_corpus_queries_qrels(tmp_path):
    _write_mini_beir(tmp_path, "mini")
    ds = load_beir_dataset("mini", cache_dir=str(tmp_path), split="test")

    # retrieval text = (title + " " + text).strip()
    assert ds.corpus["d1"] == "Postgres database system"
    assert ds.corpus["d2"] == "Cooking bread recipe"
    assert ds.corpus["d3"] == "no title doc"  # empty title collapses cleanly

    # queries filtered to those present in the split's qrels (q9 dropped)
    assert set(ds.queries) == {"q1", "q2"}
    assert ds.qrels["q1"]["d1"] == 1
    assert ds.qrels["q2"]["d2"] == 2  # graded score parsed as int


def test_load_beir_limit_queries_keeps_first_n_stable(tmp_path):
    _write_mini_beir(tmp_path, "mini")
    ds = load_beir_dataset("mini", cache_dir=str(tmp_path), limit_queries=1)
    assert set(ds.queries) == {"q1"}  # first by sorted qid
    assert set(ds.qrels) == {"q1"}    # qrels restricted in lock-step
    assert len(ds.corpus) == 3        # corpus is NEVER subsampled


def test_load_beir_missing_file_raises(tmp_path):
    d = tmp_path / "beir" / "broken"
    (d).mkdir(parents=True)
    (d / "corpus.jsonl").write_text('{"_id":"d1","title":"a","text":"b"}\n')
    (d / "queries.jsonl").write_text('{"_id":"q1","text":"a"}\n')
    # no qrels/test.tsv — corpus.jsonl exists so no download is attempted; must fail loud.
    with pytest.raises(FileNotFoundError):
        load_beir_dataset("broken", cache_dir=str(tmp_path))


# --- CachedOpenAIEmbedder --------------------------------------------------------------------------
def test_cached_embedder_lookup_after_warm(tmp_path, monkeypatch):
    emb = CachedOpenAIEmbedder(model="fake", dim=3, cache_dir=str(tmp_path))
    # deterministic fake embedding (no network): vector encodes len(text) in the first slot.
    monkeypatch.setattr(emb, "_fetch_embeddings",
                        lambda texts: [[float(len(t)), 1.0, 2.0] for t in texts])
    emb.warm(["a", "bb"])

    fn = emb.as_embed_fn()
    assert fn("a") == [1.0, 1.0, 2.0]
    assert fn("bb") == [2.0, 1.0, 2.0]

    with pytest.raises(KeyError):  # un-warmed text: loud, never a silent zero vector
        fn("never-warmed")


def test_cached_embedder_no_zero_vectors(tmp_path, monkeypatch):
    emb = CachedOpenAIEmbedder(model="fake", dim=3, cache_dir=str(tmp_path))
    monkeypatch.setattr(emb, "_fetch_embeddings", lambda texts: [[0.0, 0.0, 0.0] for _ in texts])

    with pytest.raises(ValueError):
        emb.warm(["z"])

    # the degenerate vector was neither cached in memory nor persisted to disk
    with pytest.raises(KeyError):
        emb.as_embed_fn()("z")
    assert not emb.cache_path.exists()


def test_embedder_missing_key_fails_loud(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    emb = CachedOpenAIEmbedder(model="fake", dim=3, cache_dir=str(tmp_path))
    # warm() must reach the real _fetch_embeddings (text not cached), which checks the key BEFORE any
    # network call and raises — so this never touches the wire.
    with pytest.raises(RuntimeError):
        emb.warm(["needs-fetch"])
