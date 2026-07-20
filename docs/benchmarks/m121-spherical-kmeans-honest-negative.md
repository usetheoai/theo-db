# M121 — IVF cosine/ip spherical k-means: HONEST-NEGATIVE (measured no-op)

**Date:** 2026-07-20 · **Box:** DO droplet (32 GB), pgrx-managed PG17, `theodb_rs` installed.
**Verdict:** spherical k-means does **not** change IVF cosine/ip recall — **reverted** per the M121 DoD
(bullet 3: *"se o lift não justificar, reverter e registrar honest-negative"*). No code shipped; this is the
investigation record.

## Question

The backlog (M49 review, council-index-storage HIGH-2) proposed **spherical k-means** — normalizing the Lloyd
centroid onto the unit sphere (Dhillon & Modha 2001) — to lift IVF cosine/ip recall (measured 0.83–0.89 vs HNSW
1.0). M121 tests that hypothesis **measurement-first**, not by assumption.

## Result: PROVABLE no-op for cosine, measured-identical for ip

### 1. Mathematical proof (cosine — dataset-independent)

For a cosine index, BOTH the k-means assignment and the scan probe-selection use the **scale-invariant**
`cosine_distance` (`theodb_rs/src/vec.rs:64` = `1 − ⟨a,b⟩ / (‖a‖·‖b‖)`):

- **Lloyd assignment** — `ivf.rs::nearest_in` → `metric.dist(v, center)` → `cosine_distance`. Scale-invariant in
  `center`: normalizing a centroid (scaling it to unit length) leaves the direction unchanged, so **every
  assignment is identical** → the clustering is identical iteration to iteration.
- **Scan probe-selection** — `ivf.rs::search` → `cosine_distance(q, centroid)`. Scale-invariant in `centroid`:
  the stored centroid magnitude does **not** affect which lists are probed.

Normalizing the centroid changes only its stored **magnitude**, which nothing downstream reads in a
scale-sensitive way. ∴ recall is **byte-identical on any dataset**. This is a proof, not a benchmark artifact.

### 2. Inner product (`<#>` = `−⟨a,b⟩`, `vec.rs:77`)

IP is **not** scale-invariant (the assignment has a magnitude bias), so the proof does not carry. But IP k-means
with an arithmetic-mean update is already degenerate (there is no bounded `argmax ⟨v,c⟩` centroid), and real IP
embeddings are near-unit-norm. The measurement below shows **no** differential effect on this data.

### 3. Measurement (same-binary GUC toggle + REINDEX — recall is deterministic, box/binary-independent)

A build-time GUC (`theodb_ivfflat.spherical_kmeans`, default off) gated the normalization purely to A/B it.
Recall@10 vs seqscan brute-force ground truth:

| Config | metric | recall MEAN (spherical off) | recall SPHERICAL (on) |
|---|---|---|---|
| N=50000, dim=128, lists=224, probes=3 | cosine | 1.0000 | 1.0000 |
| N=50000, dim=128, lists=224, probes=3 | ip | 1.0000 | 1.0000 |
| N=10000, dim=128, lists=100, probes=1, held-out queries | cosine | 1.0000 | 1.0000 |
| N=10000, dim=128, lists=100, probes=1, held-out queries | ip | 1.0000 | 1.0000 |

**mean ≡ spherical in every configuration**, for both metrics. No lift → nothing to ship.

### Honest methodology caveat

The synthetic uniform dataset did not reproduce a sub-1.0 absolute baseline: an EXPLAIN confirmed the index IS
exercised (`Index Scan using t121_cos ... Order By: (v <=> ...)`, not a seqscan fallback), yet a **probes sweep
gave `recall@probes=1` == `recall@probes=100` == 1.0000** — the uniform 128-d corpus is trivially separable, so
the list-coverage gap the backlog measured on real embeddings (SIFT-class) did not appear here. This does **not**
weaken the cosine conclusion (which is a scale-invariance *proof*, independent of the absolute recall level), and
the *differential* (mean vs spherical) — the only thing M121 tests — is zero in every run. Reproducing the
absolute 0.83–0.89 baseline would need a real-embedding dataset; it is not needed to settle M121.

## Decision (DoD bullet 3)

**Reverted** the spherical implementation (the GUC + the `ivf.rs` centroid normalization + the two build-site
wirings) to byte-identical. Shipping a default-off GUC that provably never improves recall would be exactly the
"config knob nobody asked for" the parsimony ladder forbids (YAGNI / KISS). The finding — spherical k-means is a
scale-invariance no-op for cosine and shows no measured effect for ip — is the milestone's deliverable. The git
history retains the apparatus for any future real-embedding re-measurement.

## Reproduction

`scratchpad/m121_recall_ab.sql` (same-session GUC-off vs GUC-on REINDEX A/B) against a build with the (now
reverted) `theodb_ivfflat.spherical_kmeans` GUC. The proof in §1 needs no run — it follows from `vec.rs::cosine_distance`.
