# ADR-0035 — M73: veredito MEDIDO do North Star vetorial vs ScaNN/AlloyDB

- **Status:** Accepted (2026-07-10)
- **Contexto:** M73 (Roadmap v5) — o head-to-head MEDIDO que emite o veredito de superioridade vetorial que o
  North Star (`docs/adr/0002`, LOCKED) exige. Depende de M60+M71+M72.
- **Natureza:** este ADR registra um **veredito medido** (onde o TheoDB está vs o SOTA), não uma mudança de
  mandato. O reposicionamento do North Star é a proposta `docs/adr/0033` — decisão do owner, separada.

## Contexto (o que o North Star exige)

O ADR-0002 (Opção α) manda **igualar OU superar o AlloyDB** no vetor, buscando **superioridade de QPS comprovada
por benchmark**. O M73 é o milestone que emite o veredito rastreável: depois de fechar o Gap 1 (M60) e de shipar o
multi-entry (M71), medir head-to-head e dizer honestamente ONDE o TheoDB está. O DoD admite explicitamente três
saídas: (a) fechou/reduziu o gap, (b) paridade own-code + trade-off documentado, (c) honest-negative.

## Evidência consolidada (measurement-first — todos com artefato)

### Eixo 1 — theodb vs pgvector (own-code vs a extensão de referência permissiva)
- **Recall:** paridade de VALOR alcançada. Gap 1 fechado (M60/ADR-0034): f32 0.974→**0.990**, SBQ 0.986→**0.994**
  a 500k (pgvector 0.994). Artefato: `docs/benchmarks/gap1-extend-candidates.md`.
- **Frontier de latência:** honesto — a iso-recall alta o theodb ainda precisa de ~1.8× o `ef` do pgvector a 500k
  (o `extendCandidates` subiu o teto de recall, não igualou a eficiência recall-por-ef). Follow-up documentado.
- **Multi-cliente (M72, 1M×128d, 8 clientes concorrentes):** `docs/benchmarks/m72-qps-multiclient.md`. A recall
  casado ~0.91, o theodb_hnsw **SUPERA** a pgvector (0.917 @ 597.7 QPS vs 0.9095 @ 539.5 QPS, +11%, p50 13.6 vs
  16.5 ms) e alcança recall (0.97 @ 354 QPS) que a pgvector platôa antes (~0.914) NESTE regime clusterizado 128d —
  o regime-alvo do extendCandidates (ADR-0034). Build também ~3× mais rápido (367 s vs 1084 s). **Honesto:** é o
  regime favorável ao theodb; o frontier de alta-dim/alto-recall (500k×768d) permanece da pgvector.

### Eixo 2 — pgvector/theodb full-precision vs ScaNN/AlloyDB (o gap de paradigma)
- **M33 (medido):** ScaNN ~**25×** QPS sobre o theodb_ivfflat full-precision (SIFT1M). Vantagem = quantização
  anisotrópica + Asymmetric-Hashing LUT SIMD (FastScan), não grafo full-precision.
- **RaBitQ (melhor quantizador permissivo do SOTA, spike D3 1M×768d):** MSTG-RaBitQ-mem 8.2ms @ 98.4% —
  **competitivo** com full-precision (~10–15ms), **NÃO** 25×. O ganho do RaBitQ é **memória** (5.3MB residentes na
  variante disk), não QPS. Artefato: `docs/benchmarks/vector-pillar-verdict-2026-07.md`. (Detalhe do lever: M74/ADR-0036.)

## Decisão (o veredito — saída (b)+(c) do DoD, honesto)

**Veredito medido do pilar vetorial P0:**

1. **Paridade own-code classe-pgvector de RECALL: ALCANÇADA** (M60/M71). O TheoDB tem tipo vetorial próprio
   (M69/M70), AM HNSW próprio, e recall de valor equivalente ao pgvector a 500k.
2. **Superioridade de QPS vetorial sobre o AlloyDB/ScaNN: NÃO-ALCANÇÁVEL** como extensão Postgres permissiva
   (honest-negative). Perseguida por todos os caminhos honestos e medida: o 25× do ScaNN é do algoritmo dele
   (AH-LUT anisotrópico, 128d, tuning de anos) + o fato de não pagar o imposto MVCC/WAL/heap que qualquer extensão
   paga. O melhor quantizador permissivo (RaBitQ) não reproduz esse gap.
3. **Trade-off documentado:** own-code + paridade de recall + throughput multi-cliente **competitivo a superior no
   regime 128d clusterizado** (M72: +11% QPS a recall casado, build 3× mais rápido), com o **frontier de
   alta-dim/alto-recall ainda da pgvector** (ADR-0034, 768d@0.99). Regime-dependente, medido, sem claim universal.

**Posicionamento permitido (per `public-copy.md`):** "paridade de recall classe-pgvector com índice vetorial
own-code" ✅; "eficiência de memória RaBitQ para billion-scale" ✅ (M74); **jamais** "mais rápido que o AlloyDB no
vetor" ❌ (não medido, refutado).

## Alternativas rejeitadas

- **Declarar superioridade** — proibido (Regra 5): nenhum benchmark a sustenta; o oposto foi medido.
- **Declarar fracasso do pilar** — desonesto na outra direção: a paridade own-code de recall É uma entrega real
  (M69/M70/M60/M71), e a fundação de memória RaBitQ (M74) é um diferencial genuíno de escala/custo.
- **Adiar o veredito esperando um lever mágico** — anti-sunk-cost (D3): já medimos os caminhos; o veredito honesto
  é o entregável, não uma vitória inventada.

## Consequências

- **Positivas:** o North Star ganha a **prova medida de onde o TheoDB está** (o que ele pede). Rastreabilidade total
  (M33/M60/M71/M72/RaBitQ, todos com artefato). Base honesta para a decisão de reposicionamento do owner (ADR-0033).
- **Custos / honestidade:** o eixo "superar o AlloyDB no QPS vetorial" é **medido como não-alcançável** por extensão
  permissiva. Isso não é falha de execução — é a fronteira do que a arquitetura (Postgres permissivo) permite. Os
  diferenciais reais do TheoDB ficam em abertura, portabilidade, model-agnosticism, AI-native/HTAP e custo/escala
  (memória RaBitQ), não em QPS vetorial puro.

## Cross-references

- Mandato (LOCKED): `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`
- Reposicionamento (proposto, owner): `docs/adr/0033-north-star-reposition-proposal.md`
- Lever RaBitQ (M74): `docs/adr/0036-m74-rabitq-conditional-lever-verdict.md`
- Gap 1 fix (M60): `docs/adr/0034-hnsw-extend-candidates-navigability.md`
- Evidência: `docs/benchmarks/m72-qps-multiclient.md`, `vector-pillar-verdict-2026-07.md`, `m60-hnsw-recall.md`
- Regras: Unbreakable Rule 3 (honestidade), Rule 5 (perf medida), `public-copy.md`
