---
type: Decision
title: ADR 0046 — Destino da árvore vendorizada rabitq/: deletar ou compilar sob feature
description: 5651 linhas vendorizadas que nunca foram compiladas e cuja documentação alega uma integração inexistente — o estado inerte e superdeclarado não é aceitável.
resource: git:f7c7b93:docs/adr/0046-rabitq-vendor-tree-deleted.md
tags: [adr, vendoring, codigo-morto, anti-sunk-cost, honestidade, rabitq]
adr_id: "0046"
adr_status: Proposed
decision_date: 2026-07-16
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0046
    resource: git:f7c7b93:docs/adr/0046-rabitq-vendor-tree-deleted.md
    title: ADR-0046 — Disposition of the inert vendored rabitq tree
    last_modified: 2026-07-16
---

Um achado convergente da auditoria de system design, marcado independentemente como zumbi de
deleção, vazamento de fronteira e risco de YAGNI.

# O problema

A árvore vendorizada pelo [ADR 0032](/decisions/0032-vendor-rabitq-rs-core.md) — **5651 linhas**,
Apache-2.0, sete arquivos, incluindo um de 119 KB — está:

- **Não compilada.** Não há declaração de módulo no crate, **zero** referências a ela em qualquer
  lugar do código compilado, e ela não aparece no manifesto nem no script de build.
- **Mal documentada.** O arquivo de proveniência alega que edições de integração reescreveram os
  imports para caminhos internos do crate, mas os arquivos **ainda usam os caminhos originais do
  crate standalone** — de modo que a árvore **nem compilaria** se fosse declarada. **A fronteira
  anticorrupção declarada é ficção.**

A vendorização em si foi decisão legítima, limpa de licença e provada por spike — as medições do
[ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md) a usaram. O problema é o **estado
congelado e inerte**: milhares de linhas que nenhum CI toca, garantindo bit-rot, mais uma
documentação que superdeclara.[^adr0046]

# Direcionadores

Anti-sunk-cost: o esforço gasto vendorizando nunca justifica manter peso morto. Código vendorizado
não compilado **deriva em silêncio** — sem compilador, sem teste, sem CI que o guarde. Honestidade: a
documentação precisa bater com a realidade. E o ADR 0036 **já decidiu** não construir o AM completo
especulativamente.

# Decisão

**Deletar a árvore**, a menos que uma feature de billion-scale nomeada e datada esteja no roadmap
dentro de um milestone — caso em que a alternativa é **compilá-la sob feature flag**, fazendo a
reescrita de caminhos documentada para que ela de fato compile, e reescrevendo a documentação para
dizer "em progresso, não conectado", com issue de rastreio.

**De um jeito ou de outro, o estado atual — inerte e superdeclarado — não é aceitável.**

# Consequências

**Deletar** remove milhares de linhas fadadas a apodrecer; o histórico do git e o ADR 0032 preservam
a proveniência para uma re-vendorização limpa depois, e a superdeclaração desaparece. O custo é
refazer o (pequeno) esforço de vendorização no futuro — aceitável sob anti-sunk-cost.

**Compilar sob feature** torna a fronteira real e testável, com o CI guardando contra rot, e torna a
intenção explícita. O custo é carregar uma feature de compilação e sua manutenção para algo ainda não
demandado — só se justifica se a demanda for genuinamente próxima.

[^adr0046]: ADR-0046 — Disposition of the inert vendored rabitq tree
