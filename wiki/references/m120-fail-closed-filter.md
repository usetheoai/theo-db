---
type: Reference
title: Filtro estruturado fail-closed para a busca híbrida (evidência de segurança)
description: Substitui uma blacklist sintática por composição em Rust com quoting de identificador e literal mais allowlist de operadores — a única opção fail-closed para chamadores não confiáveis.
resource: git:f7c7b93:docs/security/m120-fail-closed-filter.md
tags: [referencia, seguranca, sql-injection, fail-closed, busca-hibrida, m120]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m120sec
    resource: git:f7c7b93:docs/security/m120-fail-closed-filter.md
    title: M120 — Fail-closed structured filter
    last_modified: 2026-07-20
---

Fecha um achado de auditoria de segurança: **a guarda do filtro SQL cru era uma blacklist sintática, não
um parser** — e blacklist sintática não é defesa contra injeção.

# O que mudou

A [busca híbrida](/features/06-busca-hibrida.md) passou a aceitar uma chave de **filtro estruturado** —
`[{col, op, value}]` — como alternativa **fail-closed** ao filtro SQL cru, que roda com privilégio do
chamador. A composição é feita em Rust:

- **Identificador** passa por quoting de identificador.
- **Valor** passa por quoting de literal para strings; números vão nus; booleanos viram `true`/`false`.
- **Operador** passa por **allowlist fixa** — `= < > <= >= <> IN &&`. Qualquer outra coisa vira erro
  tipado.
- **O filtro estruturado e o filtro cru são mutuamente exclusivos**: os dois juntos viram erro.

O filtro cru é **retido** como escape hatch opt-in e documentado, **com privilégio de chamador e sem
alegação de ser seguro contra injeção — ele nunca foi**.

# Evidência A/B, reproduzível dentro do banco

| # | Teste | Resultado |
|---|---|---|
| 1 | **Paridade** entre estruturado e SQL cru para o mesmo predicado | 20 linhas em ambos, interseção 20 — **paridade confirmada** |
| 2 | **Operador fora da allowlist** (`"; DROP TABLE h; --"`) | **rejeitado** com erro tipado |
| 3 | **Os dois filtros juntos** | **rejeitado** com erro tipado |
| 4 | **Valor de injeção** (`"1); DROP TABLE h; --"`) | citado como literal → **a tabela SOBREVIVE**; o DROP nunca executou |

O teste 4 é o que importa: não basta rejeitar o ataque, é preciso demonstrar que o payload **passou pelo
caminho e não fez nada** — a diferença entre uma defesa provada e uma esperada.

# Fronteira honesta

- **O filtro estruturado é menos expressivo** que SQL cru — sem subqueries, sem funções. **Isso é o
  ponto**, não uma limitação a corrigir: a expressividade é exatamente o que se troca por ser
  fail-closed.
- A allowlist de operadores é extensível com uma linha e um teste.
- Isto endurece **apenas a composição do filtro relacional**. As guardas de SSRF do caminho de HTTP são
  outras, e não mudaram.

# O que a revisão encontrou depois

A auditoria confirmou que o caminho estruturado é **completo** contra injeção — nenhum byte dele alcança
o SQL cru sem passar por quoting ou pela allowlist. **E encontrou um fail-open de forma que o A/B não
havia testado**, corrigido junto.

Esse detalhe vale registrar como método: **um A/B que passa não prova ausência de defeito na dimensão que
ele não exercitou.** É a mesma lição do [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md), em
outro domínio.

# Relacionado

A postura de segurança da superfície de IA, incluindo SSRF e prompt injection, está em
[funções generativas em SQL](/guides/sql-ai-functions.md) e no
[ADR 0043](/decisions/0043-m102-ai-operators-batched-pushdown.md).
