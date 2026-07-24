# M147 — Benchmark: refactor de `am/scan.rs` byte-idêntico + QPS neutro

**Data:** 2026-07-24 · **Substrato:** droplet e2e-runner (PG 18.4 + pgrx 0.19) · **Metodologia:** A/B do M145.

## O que foi refatorado (comportamento preservado)

Três eixos de `theodb_rs/src/am/scan.rs` (issue #170, consenso 5-pilares), **sem violar a ADR-2 do M145**:

1. **Bullet 1** — if-ladder de 5 predicados `ivf_is_v*` (cada um relendo o bloco 0) → `enum IvfVersion` lido uma vez + `match` exaustivo (OCP).
2. **Bullet 2** — os 8 gather helpers `Vec` → `Result + ?` com um único boundary de erro (`enum ScanError`), no lugar de ~56 `match { Ok=>v, Err=>ereport }` C-style.
3. **Bullet 3** — o kernel Stage-1 (`ah_score_block` + loop de blocos) copiado byte-a-byte em 5 corpos → `stage1_score_blocks` compartilhado que **recebe `codes_off` do chamador** (o decode on-disk por-versão permanece separado — ADR-2).

`scan.rs`: **1567 → 1400 linhas** (−167 líquidas).

## Evidência 1 — comportamento byte-idêntico nos 6 caminhos (v3..v8)

`ab_scan_versions.sh` constrói um índice IVF em cada versão com dataset determinístico (hash de Knuth por (g,d), 2000 vetores dim-8), roda 5 queries fixas (k=10) e diffa o top-k (id:dist arredondada) contra o baseline capturado do binário pré-refactor (`m147-ab-baseline.txt`).

| Fase | Resultado |
|---|---|
| Fase 1 (enum) | `AB_COMPARE_OK` — 6 caminhos byte-idênticos |
| Fase 2 (Result+?) | `AB_COMPARE_OK` — 6 caminhos byte-idênticos |
| Fase 3 (kernel) | `AB_COMPARE_OK` — 6 caminhos byte-idênticos |

**Não-vacuidade (o A/B detecta regressão):** mutar `codes_off` no kernel (`8n` → `8n+1`) no caminho v5 → `AB_COMPARE_FAIL`; restaurado → `AB_COMPARE_OK`. O gate prova o vazamento de layout que a ADR-2 barra.

## Evidência 2 — taxonomia de erro preservada (bullet 2)

O `Result + ?` preserva a classe de erro do M146 **por construção** (51 sítios → `Corrupt`/XX002; 5 `build_lut16` → `codebook_dim`-condicional):

| Caso | SQLSTATE | Local |
|---|---|---|
| query com dimensão errada num índice íntegro | `22023` (invalid_parameter_value) | `pg.rs:44` (err_input) |
| corrupção de bytes do índice | `XX002` (index_corrupted) | `pg.rs:15` (err_corrupt), backend ALIVE 400 |
| scan sem `ORDER BY` (guard #177) | `22023` | preservado |

## Evidência 3 — QPS neutro (DoD: regressão ≤ 5%)

`qps_bench.sh`: 200 queries × 5 runs (warm-up descartado) sobre um índice v5, mean em ms (menor=melhor). Baseline = binário pré-refactor (`6e648fa`), Novo = pós-3-fases (`cba5ecf`).

| Binário | run 1 | run 2 | run 3 | média |
|---|---|---|---|---|
| baseline (pré-refactor) | 387 | 383 | 374 | **~381 ms** |
| novo (pós-refactor) | 383 | 358 | 391 | **~377 ms** |

**Δ ≈ −1% (o novo é ligeiramente mais rápido)** — dentro do ruído, muito abaixo do teto de 5%. Esperado: o dispatch lê o bloco 0 uma vez em vez de 5×; o kernel é `#[inline]`. Zero regressão.

## Reprodução

```bash
# no droplet (PG 18.4 + pgrx 0.19), extensão instalada:
cd theodb_rs/isolation
bash ab_scan_versions.sh compare docs/benchmarks/m147-ab-baseline.txt   # byte-identidade
bash qps_bench.sh novo                                                  # QPS
# taxonomia: scripts/cassert-smoke.sh (dim-errada→22023, corrupção→XX002, guard #177, 5 probes de injeção)
```
