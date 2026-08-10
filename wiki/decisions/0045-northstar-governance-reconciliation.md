---
type: Decision
title: ADR 0045 — Reconciliar o north star LOCKED com o veredito vetorial medido
description: ADR de governança, sem mudança de código: o mandato de registro ficou atrás da evidência, e ADRs posteriores já citavam o reposicionamento como se assinado.
resource: git:f7c7b93:docs/adr/0045-northstar-governance-reconciliation.md
tags: [adr, governanca, north-star, divida-documental]
adr_id: "0045"
adr_status: Proposed
decision_date: 2026-07-16
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0045
    resource: git:f7c7b93:docs/adr/0045-northstar-governance-reconciliation.md
    title: ADR-0045 — Reconcile the LOCKED North Star
    last_modified: 2026-07-16
---

Um ADR de **governança**: recomenda reconciliar registros de decisão, não mudar código. Nasceu de
uma auditoria de system design, que o marcou como o único trade-off do repositório com racional
inválido.

# O problema

O [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md), **LOCKED**, manda perseguir
superioridade de QPS vetorial sobre o [AlloyDB](/technologies/alloydb.md)/[ScaNN](/technologies/scann.md).
O time então **mediu** essa meta e registrou o resultado:

- [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md): a superioridade **não é alcançável**
  como extensão PostgreSQL permissiva — o gap de 25 a 44× a recall 0,99 é de **paradigma**, não de
  tuning.
- [ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md): o melhor lever permissivo
  compra **memória, não QPS**, e o time deliberadamente **não** construiu o AM completo.

O registro de reposicionamento, o [ADR 0033](/decisions/0033-north-star-reposition-proposal.md), foi
redigido para atualizar o mandato — mas ficou como *proposto* enquanto milestones posteriores
embarcaram **citando o posicionamento dele como se já adotado**.

**O mandato de registro e a realidade medida ficaram em contradição documentada e não resolvida.** Um
engenheiro lendo o repositório não conseguiria dizer se "superar o AlloyDB no QPS vetorial" ainda era
meta viva.

# Direcionadores

O protocolo de mudança exige **assinatura do owner** para alterar um ADR LOCKED; uma auditoria não
pode virar um mandato travado unilateralmente. Enquanto isso, ADRs posteriores já assumem a moldura
reposicionada — o registro está **de facto adotado e de jure não assinado**, e o drift compõe a cada
milestone. E a disciplina de copy pública **proíbe** o claim que o documento travado ainda implica
ser a meta.

# Decisão

**Assinar o ADR 0033** (adotar o reposicionamento), com o fallback mínimo de **acrescentar nota
explícita de supersede ao ADR 0002** apontando para os vereditos medidos, deixando o resto intacto.
Qualquer um dos dois fecha a dívida de governança; o primeiro é preferível porque os ADRs posteriores
já o referenciam. **Nenhuma mudança de código** decorre — o engine já reflete a realidade medida.[^adr0045]

# Consequências

O mandato de registro passa a bater com a evidência medida e com o posicionamento embarcado, novos
contribuidores leem um north star consistente, e o risco de copy pública some. Além disso, preserva a
disciplina anti-sunk-cost como postura **documentada**, e não implícita.

**Custo:** exige um ciclo de decisão do owner e estreita formalmente o escopo do que pode ser
alardeado — que é justamente o resultado honesto, já que a ambição foi medida e delimitada.

**Neutro:** o [ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) **não é afetado** — seu
racional (paridade de recall, tipo vetorial próprio, moat de escala em memória) nunca se apoiou em
superioridade de QPS, então o veredito negativo não o invalida.

[^adr0045]: ADR-0045 — Reconcile the LOCKED North Star with the measured vector verdict
