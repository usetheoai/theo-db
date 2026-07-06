# M51 — SBQ-inline no `theodb_hnsw`: recall×QPS (gate D3)

**Date:** 2026-07-06 · **Milestone:** M51 · **Metric:** cosine (`<=>`) · **GT:** seqscan exato · **Image:** `theodb:m51`
**Harness:** `benchmarks/run_m51_sbq_inline.py` (reusa `theodb_bench.metrics`, espelha o M50) · **n=25 000, dim=128, k=10, 3 runs**
**Verdict (D3):** **Read path CORRETO — recall gate ≥0.99 ATINGIDO (0.9993); mas o ganho de QPS NÃO se materializa a 25k (sem pressão de memória — exatamente o que o M50 previu).** RETÉM a implementação (opt-in, default off); o claim ≥2× QPS fica como follow-up rastreado em escala com pressão de memória.

---

## ⚠️ Caveats (Rule 3)

Escala reduzida (25k×128 gaussiano) numa box contendida (`load_pre=7.53`, por-run `[6.37, 7.7, 10.4]`) — **decisão do usuário 2026-07-06** (a box não roda 1M×3-builds; provado no M50). Números ABSOLUTOS de latência carregam ruído; a leitura RELATIVA (recall preservado; SBQ não mais rápido que f32 a esta escala) é robusta (consistente nos 3 runs, `recall_std ≤ 0.006`). O claim de superioridade de QPS ≥2× **só é mensurável em escala com pressão de memória** (M50 § veredito) — **follow-up rastreado**, não vendido como cumprido aqui.

## 1. Curvas recall × QPS (1 cliente, cosine, mean±std sobre 3 runs)

`theodb_hnsw_sbq` = layout v2 (códigos SBQ 8-bit inline + rerank f32), knob = `over_fetch` @ ef_search=400.
`theodb_hnsw_f32` = layout v1 (f32), knob = `ef_search`. `pgvector_hnsw` = baseline SOTA permissiva.

| index | knob | recall@10 | p50 (ms) | qps (1c) | build (s) |
|---|---|---|---|---|---|
| **theodb_hnsw_sbq** | of=2 | 0.9460 ± 0.006 | 11.64 | 93.1 | 8.6 |
| **theodb_hnsw_sbq** | of=4 | 0.9833 ± 0.003 | 19.02 | 59.2 | 8.6 |
| **theodb_hnsw_sbq** | of=8 | 0.9953 ± 0.001 | 31.10 | 38.3 | 8.6 |
| **theodb_hnsw_sbq** | **of=16** | **0.9993 ± 0.001** | **41.68** | 26.9 | 8.6 |
| theodb_hnsw_f32 | ef=200 | 0.7973 ± 0.008 | 5.91 | 173.4 | 11.0 |
| theodb_hnsw_f32 | **ef=400** | **0.9320 ± 0.007** | **10.48** | 95.5 | 11.0 |
| pgvector_hnsw | ef=200 | 0.8213 ± 0.012 | 4.44 | 229.6 | 6.4 |
| pgvector_hnsw | **ef=400** | **0.9467 ± 0.008** | **7.24** | 142.0 | 6.4 |

## 2. Leitura

- **Recall gate ≥0.99: ATINGIDO.** O SBQ-inline (Hamming no walk + rerank f32 exato) chega a **recall@10 = 0.9993** (of=16) / 0.9953 (of=8). **É o único spec que ultrapassa 0.99** — f32 e pgvector topam em ~0.93–0.95 no ef=400 máximo. Isso é uma **vantagem de recall** do rerank exato sobre um pool largo, e valida a predição do M40 (recall é carrier-limited: carrier adequado → rerank recupera ~1.0). *(O config 2-bit/ef=100 topa em ~0.52 — honest-negative: a navegação Hamming precisa de bits + carrier adequados; ver §3.)*
- **QPS a 25k: SBQ NÃO é mais rápido.** No recall casado ~0.95: SBQ (of=2) **0.946 @ 93.1 qps** ≈ f32 (ef=400) **0.932 @ 95.5 qps** — **paridade**, ambos atrás do pgvector (142). No gate ≥0.99, o SBQ custa QPS (38→27) porque o over_fetch alarga o pool e o rerank re-lê mais páginas. **Isso é esperado e honesto**: a 25k o corpus f32 cabe em RAM (~12,8 MB) → a compressão 4× do SBQ 8-bit **não tem pressão de memória onde ganhar QPS**; ela só paga o custo do walk aproximado + rerank. **Exatamente o veredito do M50** (§4): "o ganho de QPS do SBQ só materializa sob pressão de memória".

## 3. Honest-negative registrado (measurement-first, TheoDB rule 5/7)

O primeiro run (config `sbq_bits=2, ef_search=100`) deu **recall@10 = 0.52** mesmo com over_fetch=8. A causa medida: a navegação do walk por Hamming em códigos 2-bit/128d é lossy demais — o NN verdadeiro não entra no pool, então o rerank não recupera. Probe subsequente (2026-07-06) mediu a recuperação: `sbq_bits=4, ef=400, of=8` → 0.980; `sbq_bits=8, ef=400, of=8` → 0.997. **O gate ≥0.99 exige bits + carrier adequados** (8-bit, ef=400, over_fetch≥8) — o custo de QPS disso é o que a §2 mostra. Nada disso é escondido; é o gate D3 fazendo seu trabalho.

## 4. VEREDITO D3 (anti-sunk-cost) + decisão keep/kill

**RETÉM a implementação SBQ-inline** — mas com o claim de QPS honestamente delimitado:

- **Correção: PROVADA.** O read path (Hamming walk + rerank f32) é correto e **preserva recall ≥0.99** (0.9993), o gate central do M51. 12 pg_test cobrem o formato v2, o build, o fold-preserve, o reloption e o recall do read path. Default `sbq_bits=0` (f32) → **zero regressão** em índices existentes.
- **QPS-superioridade: NÃO demonstrada a 25k** (parity-to-slower vs f32) — consistente com o M50. **NÃO é um kill**: o código é correto, opt-in, sem regressão; o benefício de QPS é uma propriedade de ESCALA (pressão de memória) que esta box não mede.
- **Condição herdada do M50 (§4):** o claim `≥2× QPS a recall≥0.99 vs pgvector` **só é mensurável em escala com pressão de memória** (≥250k @1536d ou 1M @768d) numa box quieta → **follow-up rastreado** (`backlog.md`), não cumprido aqui.

**ADR keep/kill do AM próprio:** ver `docs/adr/0015-sbq-inline-keep-kill.md` — critério registrado de quando o AM próprio deixa de valer (se, medido em escala com pressão de memória, o SBQ-inline seguir ≤ pgvector+diskann no Pareto realista, reabrir a decisão de composição).

## 5. Metodologia / reprodução

```bash
PGPORT=<port> PGOPTIONS='-c statement_timeout=300000' \
  python3 benchmarks/run_m51_sbq_inline.py --n 25000 --dim 128 --nq 50 --runs 3 --out m51.json
```
Imagem `theodb:m51` (theodb_hnsw v2 SBQ + pgvector no mesmo processo). GT = seqscan exato cosine. Índices isolados por spec. Raw completo em `m51-sbq-inline.json`.
