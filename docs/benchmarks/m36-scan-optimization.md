# M36 — Otimização do scan do índice: heap top-K lazy (Phase 1)

**Dataset:** sintético 200.000×128 (distintos, seed 42) · **Hardware:** i7-1355U · 12 cores · 15.3 GB · AVX2 —
CPU móvel, single-thread, **thermal-throttled com variância alta run-to-run** (números absolutos subestimam um
servidor) · **k=10**, 8 runs (mean±std), 200 queries.

## Contexto (measurement-first FALSIFICOU a premissa original)

O gate measurement-first do M36 (`THEODB_SCAN_PROFILE=1`) mostrou que a **distância full-precision é ~14–15%** do
custo de scan — **não o gargalo**. Os gargalos medidos são **`reads` (I/O de página) ~44–51%** e **`sort`
(ordenar TODOS os candidatos) ~35–41%**. O milestone foi re-escopado (ADR-1 do blueprint). **Phase 1 ataca o
`sort`.**

## A mudança (Phase 1)

Substituí o `results.sort_by` O(C·log C) de TODOS os candidatos (`am/scan.rs`) por um **heap min lazy**: heapify
O(C) no `amrescan` + pop O(log C) por `amgettuple`. O executor puxa ~k vezes para um `LIMIT k`, então o custo total
é **O(C + k·log C)** em vez de **O(C·log C)**. O top-K emitido é **byte-idêntico** ao sort → **recall inalterado**.

## O achado ESTÁVEL (algorítmico): a fase sort colapsou

Profiler (probes=50, 50.332 candidatos, `THEODB_SCAN_PROFILE=1`): a fase do lado do `amrescan` caiu de
**~10.000–15.000µs (sort, M35)** para **~760–1.130µs (heapify, M36)** — **~10–13× menos** naquela fase. Isto é
algorítmico e robusto (O(C·log C) → O(C); o custo por-pop O(log C)×k migrou para o `amgettuple`, limitado pelo
LIMIT do executor).

## End-to-end (QPS a `ORDER BY <-> q LIMIT 10`, recall idêntico)

| probes | M35 sort (mean±std) | M36 heap (mean±std) | speedup (mean) |
|---|---|---|---|
| 10 | 526.6 ± 46.6 | 882.6 ± 66.2 | **1.68×** |
| 50 | 112.5 ± 3.8 | 162.4 ± 16.1 | **1.44×** |
| 100 | 47.1 ± 4.4 | 81.2 ± 6.8 | **1.72×** |

O heap ganha em todos os probes, num **band de ~1.4–1.7×** (mean sobre 8 runs). **A variância é alta** (CPU
throttled) — o ratio por-probe **não é limpo/monotônico**; trate como ~1.5×, não como um número preciso por-ponto.

**Reconciliação com o teto de Amdahl (honesto):** o `sort` é ~37–41% do scan; removê-lo limita o speedup de scan
a ~1/(0.6) ≈ **1.5–1.7×** — e o mean medido senta nesse teto (um pequeno extra vem da troca do comparador
`partial_cmp+unwrap` → `total_cmp` e da alocação). Ou seja: o end-to-end **não pode** ser muito maior que ~1.7×
enquanto `reads` (~44%) permanece — o que **motiva o Phase 2**. (Uma medição best-of-N anterior chegou a mostrar
~2.07×; era ruído de melhor-caso num CPU throttled — o mean é o número honesto.)

**recall idêntico** — por construção: `Scored` ordena por (`f64::total_cmp`, `tid`), a MESMA ordem total que o
`results.sort_by((partial_cmp, tid))` produzia sobre distâncias L2 finitas → top-K byte-idêntico. Provado pelo
`heap_pops_same_order_as_sort_with_ties` (`#[pg_test]`) + a suíte de 61 testes de coexistência passando inalterada
(o subconjunto de comparação de kNN ids retorna os mesmos ids; a suíte inclui também testes negativos/privilégio).
*Exceção honesta:* no caminho de scan legado (blob M26), empates exatos de distância com `pending` vazio
normalizam o desempate de posição-do-corpus para `tid` — uma ordem mais determinística, não uma mudança de recall;
a identidade byte-a-byte é exata nos caminhos estruturados (os que shippam).

## Veredito honesto

- **Fase sort fechada** (~10–13×, algorítmico/estável).
- **End-to-end ~1.5× band** a recall idêntico (Amdahl-limitado por `reads`; variância alta no CPU throttled).
- **Fecha o gap do ScaNN?** **Parcialmente** — ataca só o `sort` (~37–41%). O `reads` (~44–51%) é o **Phase 2 do
  M36** (códigos quantizados menores). O gap total (~25×, M33) também exige poda de candidatos. **NÃO "25× do
  sort sozinho".**

## Reprodução

```
# em cada imagem (theo-db:m35 = sort, theo-db:m36 = heap), no mesmo host:
PGPORT=<port> python3 benchmarks/run_m36_scan.py --n 200000 --probes 50 --k 10 --runs 8 --label <sort|heap>
```
As duas imagens diferem por EXATAMENTE um commit de engine (`8e2bdab`, só `am/scan.rs`) desde a v0.32.0
(`git diff 37c9765..8e2bdab -- '*.rs'`).
