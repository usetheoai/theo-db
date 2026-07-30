---
type: Index
title: TheoDB — conhecimento operacional de engenharia
description: Cada erro, técnica, invariante, medição e negativo honesto que este projeto pagou para aprender, num bundle OKF navegável por agente e por humano.
resource: https://github.com/usetheodev/theo-db
tags: [theodb, engenharia, metodo, okf]
timestamp: 2026-07-30T00:00:00Z
---

# TheoDB — conhecimento operacional

Este bundle existe porque o projeto vinha **repetindo classes de erro** que já havia pago. O padrão que motivou
sua criação é literal: em uma única sessão, seis diagnósticos meus foram derrubados por medição — e os que mais
escaparam foram exatamente os que **me convinham**.

O conhecimento estava espalhado por ~67 arquivos de memória, 110 blueprints, notas de implementação e mensagens
de commit. Espalhado, ele não morde no momento em que seria útil. Consolidado num formato que agentes navegam,
ele morde.

> **CORRIGIDO 2026-07-30 após review.** Esta porta de entrada declarava `type: OKF Bundle` — um **sexto tipo
> fora da taxonomia LOCKED do § 2 do contrato, sem o ADR que ele exige** — e nomeava os cinco tipos em
> `minúsculo-hífen`, convenção que **nenhum arquivo do bundle usa**: filtrar por `type: failure-mode` devolvia
> **0 de 17**. O `check_okf.py` não pegava porque o C1 valida a **presença** de `type`, não o valor.

## Formato

[Open Knowledge Format v0.1](https://github.com/google/open-knowledge-format) — um diretório de markdown com
frontmatter YAML. **Cada conceito é um arquivo**, e o caminho do arquivo é a identidade do conceito. Os links
entre conceitos são markdown normal, o que torna o bundle um **grafo**, não uma lista. O único campo obrigatório
é `type`.

## Os cinco tipos, e a pergunta que cada um responde

| Tipo | A pergunta | Onde |
|---|---|---|
| `Failure Mode` | "estou prestes a cometer isto?" | [failure-modes/](failure-modes/index.md) |
| `Technique` | "qual é o método certo aqui?" | [techniques/](techniques/index.md) |
| `Invariant` | "a plataforma permite isso?" | [invariants/](invariants/index.md) |
| `Measurement` | "isso já foi medido?" | [measurements/](measurements/index.md) |
| `Honest Negative` | "isso já foi tentado e refutado?" | [honest-negatives/](honest-negatives/index.md) |

## A regra que gerou o bundle

> **Nenhuma alegação entra em documento ou código antes de eu reproduzir a medição que a sustenta.**
> Vale igualmente para as alegações que me contradizem e para as que me favorecem — e é a segunda metade que
> falha na prática.

Adotada em [nenhuma-alegacao-sem-medicao](techniques/nenhuma-alegacao-sem-medicao.md) depois de quatro rodadas
consecutivas em que a *correção* de um defeito introduzia outro.

## Como usar

- **Antes de medir qualquer coisa:** leia [failure-modes/index.md](failure-modes/index.md). Metade das entradas
  ali são medições que pareciam válidas e não eram.
- **Antes de publicar um número:** [technique/gate-de-nao-vacuidade](techniques/gate-de-nao-vacuidade.md) e
  [measurement/index](measurements/index.md) — o número pode já existir.
- **Antes de propor uma aposta técnica:** [honest-negatives/index.md](honest-negatives/index.md). Várias já
  foram medidas e refutadas com artefato.
- **Antes de mexer em storage/FFI/recovery:** [invariants/index.md](invariants/index.md).

Histórico cronológico em [log.md](log.md).
