---
type: Decision
title: ADR 0002 — North Star: igual ou superior ao AlloyDB (Opção α)
description: Mandato LOCKED de paridade com o AlloyDB para usuários OSS/on-prem, com superioridade de performance vetorial perseguida sob benchmark — cláusula depois refutada por medição.
resource: git:f7c7b93:docs/adr/0002-north-star-equal-or-superior-to-alloydb.md
tags: [adr, estrategia, north-star, alloydb, locked, measurement-first]
adr_id: "0002"
adr_status: Accepted (LOCKED, parcialmente superseded)
decision_date: 2026-06-27
owner: human:paulohenriquevn
superseded_in_part_by: ["0006", "0033", "0035", "0036"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0002
    resource: git:f7c7b93:docs/adr/0002-north-star-equal-or-superior-to-alloydb.md
    title: ADR 0002 — North Star
    author: human:paulohenriquevn
    last_modified: 2026-07-16
---

A **fonte de verdade da estratégia de produto**, e o ADR mais consequente do repositório.
Mudá-lo exige sign-off explícito do CTO mais nota de supersede — o mesmo padrão de lock das
golden rules. Todos os demais documentos resumem e apontam para cá.

# Contexto

Mandato do CTO: *"não importa o esforço ou a complexidade — quero um banco igual ou superior ao
[AlloyDB](/technologies/alloydb.md)"*. A confirmação pilar-a-pilar produziu uma verdade
desconfortável: a estratégia *measurement-first* é necessária e correta, mas "igual ou superior"
**literal em cada interno** esbarra num teto que esforço não fura — **licença**. As peças que
igualariam o columnar *in-memory* do AlloyDB (Citus, Hydra, ParadeDB) são AGPL e portanto
barradas; e a HA por storage desagregado do AlloyDB é arquitetura de nuvem distinta.

# Decisão — Opção α

O TheoDB busca ser igual ou superior ao AlloyDB **para os seus usuários-alvo** (OSS, on-prem,
edge, model-agnostic), em três eixos:

1. **Paridade de capacidades e resultados** nos pilares onde o AlloyDB compete — entregar o
   mesmo *resultado* ao usuário, com peças permissivas.
2. **Superioridade estrutural, já hoje, sem benchmark:** abertura (Apache-2.0, auditável),
   custo (sem licença por vCPU), portabilidade (mesma imagem laptop→bare-metal) e
   **independência de modelo** (qualquer modelo local ou remoto, contra o lock-in do Gemini).
3. **Superioridade de performance no pilar vetorial**, perseguida e comprovada por benchmark
   reproduzível — nunca afirmada sem evidência.

## Doutrina operacional (LOCKED)

- **Measurement-first.** O harness reproduzível de recall@k, latência, QPS, build e memória é
  pré-requisito de qualquer claim de performance e do gatilho de fork. Construí-lo foi
  declarado a maior alavanca do programa.
- **Fork é condicional.** Não se forka `pgvector`/`pgvectorscale` antes do benchmark de
  gatilho. Forkar antes de medir é a complexidade acidental que o projeto proíbe.
- **Rota de superioridade no índice.** O *algoritmo* [ScaNN](/technologies/scann.md) é
  Apache-2.0 (só a integração do AlloyDB é fechada). As apostas seriam: adotar
  [pgvectorscale](/technologies/pgvectorscale.md) as-is → forkar → ScaNN-as-PG-AM.
- **Esforço ≠ Complexidade.** Esforço alto é bem-vindo; complexidade desnecessária é proibida
  sempre; esforço nunca justifica claim sem benchmark.

## Postura por pilar

| Pilar | Postura | Observação |
|---|---|---|
| Compat · Segurança · Deploy · Migração | Paridade alcançável | — |
| Vetorial/IA (killer) | Paridade/superioridade "vencível" | gated em benchmark |
| Abertura · custo · portabilidade · model-agnostic | **Superior hoje** | estrutural, OSS |
| Columnar/HTAP | Aposta diferente, competitiva | lakehouse, **não** in-memory — forçado pela barreira AGPL |
| HA/DR | Aposta diferente, competitiva | não é storage desagregado |
| Control plane gerenciado | Fora do v1 | — |

# O teto de licença — a Opção β está fora de escopo

Igualar *literalmente* o columnar in-memory e o storage desagregado exigiria aceitar AGPL (o
que envenena o Apache-2.0 da distribuição) ou construir esses componentes do zero — um programa
multi-anos. Essa é a **Opção β**, fora de escopo até um ADR futuro. O mandato de "esforço sem
limite" não dissolve a restrição de licença: esforço não torna AGPL seguro na distribuição.[^adr0002]

# O que a medição fez com este ADR

Duas cláusulas foram alteradas por evidência, e é isso que torna este ADR o caso exemplar da
doutrina measurement-first — ele foi usado contra si mesmo:

- **"Compor > construir"** deu lugar a "construir código próprio em Rust/Go" no
  [ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md), com measurement-first
  preservado.
- **A superioridade de QPS vetorial foi MEDIDA como não-alcançável** por uma extensão PG
  permissiva. O veredito do [M73](/benchmarks/m73-headtohead-verdict.md), formalizado no
  [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md), e a refutação da alavanca
  RaBitQ no [ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md), estabelecem
  um teto de **paradigma** (~25–44× vs ScaNN em recall alto). O reposicionamento formal está
  no [ADR 0033](/decisions/0033-north-star-reposition-proposal.md).

O restante do ADR — no-fork do engine, teto de licença, Opção β fora de escopo, honestidade —
permanece em vigor.

# Honestidade (LOCKED)

"Igual ou superior ao AlloyDB" aparece em documentos públicos como **missão**, nunca como claim
de performance não-qualificado. Superioridade de performance só é afirmada com benchmark
reproduzível publicado.

[^adr0002]: ADR 0002 — North Star: igual ou superior ao AlloyDB
