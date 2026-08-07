---
type: Reference
title: Auditoria de licenças do pacote — zero AGPL na distribuição
description: A evidência reproduzível do gate de release: varredura determinística sobre pacotes do sistema e sobre a árvore de crates fixada, com o falso positivo conhecido explicado.
resource: git:f7c7b93:docs/packaging/license-audit.md
tags: [referencia, licenca, agpl, gate-de-release, auditoria, sbom]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: licaudit
    resource: git:f7c7b93:docs/packaging/license-audit.md
    title: License audit — TheoDB core package
---

> **Nota de contexto histórico.** Esta auditoria foi feita quando a distribuição ainda embarcava
> [pgvector](/technologies/pgvector.md), [pgvectorscale](/technologies/pgvectorscale.md) e `plpython3u`.
> Todos saíram depois ([ADR 0029](/decisions/0029-m70-drop-pgvector.md)), o que **reduz** a superfície
> auditada. **O método continua sendo o contrato.**

**Resultado: ZERO AGPL na distribuição**, reproduzível por um script que **sai com código diferente de
zero** em qualquer achado real de AGPL. Este é o gate de release da política de licença permissiva.

# Os três eixos varridos

**(a) Pacotes do sistema.** A varredura dos arquivos de copyright acusa **um único match**, que é
**falso positivo conhecido**: o texto de licença de um pacote de certificados **enumera** a AGPL pelo
nome dentro de uma prosa de tri-licenciamento, sem estar sob ela.

Registrar o falso positivo é parte do valor da auditoria: sem isso, cada nova execução recria a mesma
dúvida.

**(b) Árvore de crates Rust linkada estaticamente.** Sobre o commit **fixado** que a imagem constrói:
**293 crates, 0 AGPL**. A distribuição é toda permissiva — MIT, Apache-2.0, BSD, ISC, Zlib, Unicode,
Unlicense —, com as entradas do próprio projeto sob PostgreSQL License.

**(c) Extensões e linguagens procedurais.** Todas sob PostgreSQL License, com dependências de sistema
cobertas por (a) e de Rust por (b).

# O desvio de ferramenta, e por que ele é defensável

O critério de pronto nomeava uma ferramenta de auditoria multi-agente. O gate foi implementado como uma
**varredura determinística, reproduzível e versionada**.

O racional: **a pergunta do gate é binária** — "há AGPL no que embarcamos?" — e um script sobre os
pacotes da imagem mais a **árvore de crates exatamente fixada** é re-executável em CI e produz artefato
estável e auditável. **Como gate de release, isso é mais forte que uma auditoria por LLM sobre uma árvore
não fixada.**

A ferramenta multi-agente permanece disponível para auditorias periódicas mais profundas, de proveniência
e similaridade — que respondem uma pergunta diferente.

# Por que isso importa tanto neste projeto

A barreira contra AGPL não é preferência de estilo: é **restrição de arquitetura**. Ela é a razão pela
qual o columnar in-memory ficou fora de escopo
([ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)), pela qual a peça BM25 SOTA foi
barrada ([ADR 0003](/decisions/0003-permissive-bm25-pg-textsearch.md)), e pela qual o colunar teve de ser
escrito do zero em clean-room ([ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md)).

Como registra o ADR 0002: **esforço não torna AGPL seguro na distribuição.**

# Dívida honesta registrada na época

A auditoria de uma árvore de crates linkada estaticamente **é obrigação de pré-release**, e não pôde ser
assumida limpa por inspeção da licença de topo do projeto — porque é o **código transitivo** que embarca.
Ver [decisão de índice](/decisions/m2-index-decision.md), onde essa obrigação foi registrada
explicitamente em vez de silenciada.
