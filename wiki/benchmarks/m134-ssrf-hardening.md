---
type: Measurement
title: m134 — fechando a classe de SSRF do endpoint de LLM
description: Um banco que faz chamadas HTTP em nome de quem escreve SQL é um motor de SSRF a menos que duas condições valham — e antes deste milestone nenhuma valia.
resource: git:f7c7b93:docs/benchmarks/m134-ssrf-hardening.md
tags: [benchmark, seguranca, ssrf, comportamento, gate-anti-silencioso, m134]
milestone: M134
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m134
    resource: git:f7c7b93:docs/benchmarks/m134-ssrf-hardening.md
    title: M134 — closing the LLM-endpoint SSRF class
    last_modified: 2026-07-21
---

# A formulação do problema

> Um banco que faz chamadas HTTP de saída **em nome de quem escreve SQL** é um **motor de SSRF**, a menos
> que duas coisas sejam verdade: **o alvo não é controlado pelo chamador**, e **endereços internos são
> recusados**.
>
> Antes deste milestone, **nenhuma das duas valia**.

Nomear a classe inteira — em vez de corrigir um vetor específico — é o que torna a correção durável.

# O gate anti-restart-silencioso

O detalhe metodológico que merece registro: **antes de confiar em qualquer leitura**, o harness assere
que **o tempo de início do servidor é posterior à data de modificação do binário**.

Ou seja: ele **prova que o binário novo foi de fato carregado**.

Sem esse gate, um restart que silenciosamente falhou produziria medições do **código antigo** — e o
artefato reportaria que a correção funciona quando ela nem estava rodando. É a classe de erro mais
embaraçosa possível num benchmark de segurança, e o gate a elimina.

# Enquadramento

**Este milestone mede comportamento — o que o banco se recusa a chamar —, não performance.** Logo a
máquina não canônica é irrelevante para a conclusão.

# Contexto

A postura resultante — apenas `http(s)`, sem seguir redirects, com erro tipado — está documentada em
[funções generativas em SQL](/guides/sql-ai-functions.md), e o raio de dano do lado de prompt injection
é tratado no [ADR 0043](/decisions/0043-m102-ai-operators-batched-pushdown.md).
