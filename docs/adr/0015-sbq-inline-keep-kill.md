# ADR 0015 — SBQ-inline no `theodb_hnsw`: keep/kill do AM próprio

**Status:** Accepted · **Date:** 2026-07-06 · **Milestone:** M51 · **Gate:** D3 (anti-sunk-cost)

## Contexto

O M51 implementou a quantização SBQ **inline** no `theodb_hnsw` (layout v2): códigos SBQ nos element tuples, o
walk do HNSW pontua por Hamming (barato), rerank exato f32 no top `k·over_fetch`. A aposta (autorizada + re-escopada
pelo veredito-gate do M50) era mudar o asymptote de QPS atacando o custo por-candidato do scan.

O M51 DoD exige um **gate D3-style**: reter SÓ se recall@10 ≥ 0.99 for preservado E o efeito for maior que a
variância; senão honest-negative + ADR mantendo f32. Este ADR registra a decisão medida.

## Evidência medida (`docs/benchmarks/m51-sbq-inline.{md,json}`, n=25k×128, cosine, 3 runs)

- **Recall gate ≥0.99: ATINGIDO.** SBQ-inline (8-bit, ef=400, over_fetch=16) → recall@10 = **0.9993**; é o único
  spec que ultrapassa 0.99 (f32/pgvector topam ~0.93–0.95). Correção do read path provada por 12 pg_test.
- **QPS a 25k: SBQ NÃO é mais rápido** — paridade-a-mais-lento vs f32 no recall casado (of=2: 0.946@93qps vs f32
  0.93@95qps); no gate ≥0.99 custa QPS (27–38 qps). **Sem pressão de memória** (o corpus f32 cabe em RAM a 25k), a
  compressão do SBQ não tem onde ganhar QPS — consistente com o veredito do M50.
- Honest-negative: o config 2-bit/ef=100 topa em recall 0.52 (navegação Hamming lossy) — o gate exige bits+carrier
  adequados.

## Decisão

**RETER a implementação SBQ-inline**, com o claim de QPS honestamente delimitado:

1. **Reter** porque: o read path é **correto e recupera recall ≥0.99** (o gate central do M51 — 0.9993, a prova de
   que a navegação aproximada por Hamming + rerank exato não perde o NN com pool adequado; **NÃO** é um teto de recall
   superior ao f32 — o 0.999 vem de varrer o SBQ até walk_ef=6400 vs ef=400 dos baselines, comparação não-casada,
   ver `m51-sbq-inline.md § 1`); é **opt-in** (`WITH (sbq_bits=N)`, default 0 = f32) → **zero regressão** em índices
   existentes.
2. **NÃO é kill** porque o benefício de QPS do SBQ é uma propriedade de **escala com pressão de memória**, não medível
   a 25k. Matar aqui seria over-reading uma calibração de escala limitada (o mesmo erro que o M50 evitou).

## Critério de reabertura (a cláusula de saída que faltava ao AM próprio — risco 4c do deep-view)

Reabrir a decisão de composição (AM próprio vs compor sobre pgvector+pgvectorscale) **SE**, medido em escala com
**pressão de memória** (≥250k @1536d ou 1M @768d, box quieta):

- o SBQ-inline seguir **≤ pgvector + diskann** no Pareto recall×QPS realista (i.e., o lever não moveu o asymptote), **E**
- nenhum outro lever pendente (co-localização de vizinhos ADR-3, LUT16 ADC) fechar o gap.

Nesse caso, o custo de manter um AM próprio (fork de rebase, superfície de crash-safety) deixa de se justificar vs
compor sobre as extensões permissivas — e a decisão D-composição é reaberta com um ADR de sucessão.

## Follow-up rastreado (não cumprido no M51 — decisão do usuário 2026-07-06)

O claim `≥2× QPS a recall≥0.99 vs pgvector` **só é mensurável em escala com pressão de memória numa box quieta** →
registrado em `knowledge-base/backlog.md`. Este ADR NÃO afirma esse ganho; afirma um SBQ-inline **correto e
recall-preservante** cujo benefício de escala está pendente de medição.

## Alternativas rejeitadas

- **Kill (manter só f32):** rejeitada — o read path é correto, opt-in e sem regressão; o benefício é de escala, não
  medível aqui. Matar descartaria trabalho correto por uma medição fora do regime-alvo.
- **Afirmar o ganho de QPS:** rejeitada (desonesto) — não foi medido no regime onde ele existe (pressão de memória).

## Cross-references

- Benchmark: `docs/benchmarks/m51-sbq-inline.{md,json}`
- Veredito-gate upstream: `docs/benchmarks/m50-sota-ruler.md § 4`
- Plano: `.claude/knowledge-base/plans/sbq-inline-am-plan.md` (ADR-4)
- Follow-up: `.claude/knowledge-base/backlog.md`
