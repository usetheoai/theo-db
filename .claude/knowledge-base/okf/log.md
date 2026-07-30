---
type: Log
title: Histórico do bundle
description: Registro cronológico de quando cada bloco de conhecimento entrou e o que o motivou.
tags: [okf, historico]
timestamp: 2026-07-30T00:00:00Z
---

# Log

## 2026-07-30 — criação do bundle

Motivador imediato: uma sessão de trabalho no M169 em que **seis** alegações minhas foram derrubadas por medição
(#219, #220 duas vezes, EC-2, "q20 nunca observado", linha fabricada do EC-1, custo do ADR-5), mais **quatro**
defeitos de instrumentação numa única medição de memória. Nenhum deles era novo em espécie — todos tinham
precedente registrado em memória do projeto, e nenhum estava num lugar que disparasse no momento certo.

Fontes consolidadas: 67 arquivos de memória do projeto (M46→M169), o desk-check do M168, as notas de
implementação do M169, e as mensagens de commit da série.

Escopo deliberadamente **não** incluído: planos, reviews, ADRs e audits históricos. Eles continuam em
`knowledge-base/`, no formato do ciclo. Este bundle é sobre **método e invariantes**, não sobre o rastro de
execução.
