---
slug: deepview-remediation
generated_by: roadmap-feature
date: 2026-07-05
status: completed
milestones_added: M47, M48, M49, M50, M51, M52, M53, M54
---

# Grill — deepview-remediation (amendment M47–M54)

## Q1 — O que é e por que AGORA?

Deep-view de trajetória (2026-07-05, 5 council agents sobre código real: vector-ann, ai-in-db,
index-storage, benchmark, research-adr) emitiu veredito consolidado **COURSE_CORRECTION_NEEDED**:
fundações certas (engine PG, Rust/pgrx, measurement-first), mas (a) 2 bugs de correctness no AM
(issues #46 unlogged INIT fork sem WAL, #47 VACUUM rewrite não-atômico), (b) o track vetorial P0
preso no asymptote "f32 full-precision em páginas" (paridade pgvector = teto da classe; gap 25×
vs ScaNN é ~8–12× quantização de scoring), (c) régua de medição descalibrada (pgvectorscale diskann
nunca medido; toda evidência é SIFT1M/128d/L2/1-cliente), (d) lacunas AI-native de lifecycle
(vectorizer, WHERE na híbrida, BM25 medido-e-não-shipado). "Agora" porque: os 5 negativos honestos
consecutivos (M36–M46) provam que o filão de micro-higiene f32 acabou — continuar nele é sunk cost.

## Q2 — Dependências

Sequencial gated (escolha do usuário): M47(deps M46, formaliza o plano em voo) → M48(deps M47) →
M49(deps M48) → M50(deps M47+M49) → M51(deps M50, GATED pelo Pareto calibrado) → M52(deps M51) →
M53(deps M19+M50) → M54(deps M19). O gate anti-sunk-cost: M51 só executa se o diagnóstico de M50
confirmar o lever; se o Pareto com diskann/dataset real mudar o quadro, a aposta muda por ADR.

## Q3 — DoD

Ver blocos M47–M54 no ROADMAP.md (cada um com DoD verificável, file:line e artefato de benchmark
obrigatório). Fontes técnicas: issues #46/#47; relatórios do conselho (agentIds a9494916/a5a2c68d/
a3e0c080/ad872986/ab8b4a51, sessão 2026-07-05); plano FU-1 (`.claude/knowledge-base/plans/
fu1-samegraph-scan-microbench-plan.md`, milestone_id: M47).

## Q4 — Riscos NOVOS

1. **Mudança de formato on-disk (M51 SBQ inline)** — layout v3 dos element tuples exige path de
   REINDEX e versionamento (precedente: v1→v2 IVF já tratado); risco de recall se o rerank
   over_fetch for mal calibrado — mitigado pelo gate D3-style (retenção só se effect>variância).
2. **Background worker pgrx (M54 vectorizer)** — território FFI novo (fila crash-safe + worker);
   revisita o ADR 0007 (per-row síncrono). Mitigação: discover primeiro (padrão pgai/Supabase),
   fila em tabela com estados tipados, retry bounded reusa `http.rs`.

## Out-of-scope cross-check

Sem colisão. M53 (BM25 leg) EXECUTA o gate de adoção já autorizado pela exceção permissiva do
ADR 0013 ("Gated para adoção; não embarcados ainda") — consistente com o out-of-scope, não conflito.

## SOTA delta

Não necessário — referências existentes cobrem (pgvector, pgvectorscale, vectorchord, paradedb,
pinecone). Discover do M52 (filtered ANN: ACORN/adaptive-filtering) decide se precisa de peer novo.

## Decisões estruturais (usuário, 2026-07-05)

- Escopo: todos os 8 milestones.
- Ordenação: sequencial gated (M50 gates M51).
- Inserção: fim de `## Milestones v2`, antes de `## Sequência e paralelismo` (roadmap v2 não tem a
  âncora `## State-of-the-art references` do template).
- M47 formalizado no roadmap (o plano FU-1 em voo já declara `milestone_id: M47`; sem header o
  cycle-release não flipa o checkbox).
