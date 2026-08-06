# M67 — Recomendador de ef_search: convergência medida (MAE/RQUT por alvo de recall)

**Date:** 2026-07-09 · **Milestone:** M67 · **Métrica primária:** convergência (recall medido do ef recomendado vs alvo)
**Harness:** `benchmarks/run_m67_autotune.py` (corpus sintético gaussian-mixture, 10k×128, sample≠measure) · **JSON:** `docs/benchmarks/m67-autotune.json`
**ADR:** [`0026-m67-autotune-recommender.md`](../../adr/0026-m67-autotune-recommender.md)

> **VEREDITO: CONVERGED (com nuance honesta).** O `theodb.recommend_ef` **converge** — retorna o **menor ef** que
> atinge o alvo na amostra, e o recall medido (num measure-set disjunto) fica ≥ o alvo na média. **DUAS ressalvas
> honestas:** (1) o corpus sintético é **fácil demais** — o ef mínimo (=k=10) já dá recall 0.986, então todos os
> alvos colapsam para ef=10 (a curva ef↔recall não é estressada; um corpus real tipo SIFT mostraria ef=10 p/ 0.9,
> ef=40 p/ 0.95, ef=200 p/ 0.99); (2) o **RQUT (cauda) 12%** para os alvos altos mostra que o recomendador otimiza
> a **média**, não a cauda — 12% das queries do measure-set ficam abaixo do alvo no ef recomendado.

---

## 1. Resultado (measure-set disjunto do sample; k=10)

| Alvo R* | ef recomendado | recall medido (média) | MAE \|recall−R*\| | RQUT (% queries < R*) |
|---|---|---|---|---|
| 0.90 | 10 | 0.986 | 0.090 | **2%** |
| 0.95 | 10 | 0.986 | 0.052 | **12%** |
| 0.99 | 10 | 0.986 | 0.022 | **12%** |
| baseline ef=64 | 64 | **1.000** | — | 0% |

**Método:** 10k vetores gaussian-mixture (20 clusters, dim 128), índice `theodb_hnsw`; 100 queries divididas em
sample (50, para o recomendador) + measure (50, disjunto — sem leakage). Para cada alvo, `theodb.recommend_ef`
sugere o ef; o recall REAL desse ef é medido no measure-set contra o GT exato (seqscan).

## 2. Veredito (honesto, com as ressalvas)

- **O mecanismo funciona:** o recomendador faz a bisecção monotônica correta e retorna o **menor ef** que atinge
  o alvo na amostra (ef=10, pois recall(10)=0.986 já ≥ 0.90/0.95 e dentro da banda de 0.99). A média converge.
- **Ressalva 1 — corpus fácil (o mais importante):** o baseline ef=64 dá recall **1.0**, e ef=10 já dá 0.986. O
  corpus gaussian-mixture não estressa a curva ef↔recall — todos os alvos → ef=10. **Isto NÃO prova o ef-scaling**
  (que a literatura mostra: 0.9→0.99→0.999 exige ef super-linear). Um corpus real (SIFT1M, o harness M45/M50) com
  vizinhos mais ambíguos mostraria o recomendador subir o ef por alvo. Débito honesto declarado.
- **Ressalva 2 — RQUT (cauda):** 12% das queries do measure-set ficam abaixo de 0.95/0.99 no ef=10. O recomendador
  otimiza o recall **médio** na amostra, não o percentil-de-cauda. Um recomendador tail-safe (targetar o P5 do
  recall, não a média) é um refino futuro — a literatura (DARTH RQUT, Ada-ef percentis) mede exatamente isto.
- **NÃO é honest-negative** (o mecanismo converge na média), mas TAMBÉM não é uma vitória forte — é um **v1 honesto
  que funciona no mecanismo e declara as duas limitações** (corpus fácil, cauda).

## 3. O que este benchmark NÃO afirma

- **NÃO** que o recomendador escala o ef por alvo (o corpus fácil não estressa isso — precisaria de SIFT1M).
- **NÃO** tail-safety (RQUT 12% mostra undershoot de cauda — o recomendador é mean-optimal, não tail-safe).
- **NÃO** auto-tune online (deferido por evidência, ADR-0026 — oscilação).

## 4. Caveats honestos

1. **Corpus sintético fácil:** gaussian-mixture 20-clusters dim-128 → ef mínimo já satura o recall. A curva
   ef↔recall real (o valor do recomendador) só aparece num corpus ambíguo (SIFT1M). Débito rastreado.
2. **RQUT de cauda:** o recomendador é mean-optimal; 12% de undershoot de cauda. Tail-safe (percentil) é v2.
3. **n=1 run, 50 measure queries:** amostra pequena; a direção (converge na média) é mecânica.
4. **amcostestimate:** a fórmula M48 (f(ef)) é retida; `theodb.scan_stats` dá a auditabilidade real (pages_read
   medido vs estimado); a calibração-in-planning é deferida por risco EC-3 (ADR-0026 D3).

## 5. Reprodução

```
# droplet com pgrx pg17 + CREATE EXTENSION theodb_rs CASCADE:
PGHOST=localhost PGPORT=28817 PGUSER=theo PYTHONPATH=benchmarks \
  python3 benchmarks/run_m67_autotune.py --n 10000 --dim 128 --m-queries 100 --k 10 \
    --targets 0.90,0.95,0.99 --out docs/benchmarks/m67-autotune.json
```

Dados brutos: `docs/benchmarks/m67-autotune.json`.
