---
ciclo: b020-b028
date: 2026-08-12
cycle: release
input_verdict: READY_TO_MERGE_WITH_FOLLOWUPS
verdict: RETIDO_POR_DECISAO_DO_OWNER
---

# Release do ciclo B-020…B-028 — preparado, retido

> **Verdict: `RETIDO_POR_DECISAO_DO_OWNER`.** Não é `BLOCKED` (nenhum gate reprovou) nem
> `PR_OPEN_AWAITING_APPROVAL` (nenhum PR novo foi aberto). O owner determinou em 2026-08-12: **um merge só,
> e só quando o banco estiver SOTA level**. O trabalho acumula em `workspace`; o PR #227 (`develop → main`,
> v0.159.0) segue aberto do ciclo anterior.

## Gates — todos verdes, todos executados

| Gate | Resultado | Onde |
|---|---|---|
| Suíte | **442 pass / 0 fail** (320,60 s) — confirmada 2× | contêiner, toolchain pinado 1.97.0 |
| Clippy `-D warnings` | **exit 0** | idem, baseline `.clippy_args` |
| `cargo fmt --check` | **0 diffs** | idem |
| **Upgrade 1.4.0 → 1.5.0** | **4/4 cenários, exit 0** | harness do projeto, install 1.4.0 original |
| `stop-validation.sh` | sem BLOCKER | — |

Os 442 são os 440 do ciclo anterior mais os 2 testes novos do B-021. Todos os gates rodaram **sem**
`rustup component add` manual — o que é, em si, a verificação prática do B-025.

### O teste de upgrade, detalhado

Executado com o install **1.4.0 original** extraído da imagem publicada `0.140.0` — verificado que ele traz
a versão ANTIGA do `explain_scan` (`a.amname = 'theodb_hnsw'`). Sem isso, o teste rodaria contra um catálogo
que já tinha a correção e não provaria nada.

| Cenário | Resultado |
|---|---|
| A — pós-upgrade ≡ instalação limpa | **OK** (289 linhas de snapshot idênticas) |
| CONV — catálogo incompleto converge | **OK** (283 → 289) |
| IDEM — rodar 2× não erra nem muda schema | **OK** (0 erros, snapshot inalterado) |
| B1 — `.so` novo sobre catálogo antigo sem `ALTER EXTENSION` | **OK** (servidor sobreviveu) |

## Conteúdo — 9 itens, e 3 deles não eram defeitos

| Corrigidos | Mortos por medição |
|---|---|
| B-021, B-022, B-023, B-025, B-026, B-027, **B-028** | **B-020**, **B-024** |

`B-028` nasceu e morreu dentro do ciclo: foi descoberto **ao executar** o followup do review.

## O que este ciclo revelou sobre os próprios instrumentos

Sete ocorrências da mesma classe — **um instrumento que responde sem medir**:

1. `cargo fmt -- --check | grep -c` imprimindo `0` porque o comando falhou (B-025)
2. O job de CI reportando "suíte reprovou" sem ter executado um teste (B-027)
3. `cargo check --lib` não compilando os testes que eu havia acabado de escrever
4. Duas medições de performance atribuídas ao produto quando eram contenção que eu criei (B-020, B-023)
5. O `explain_scan` devolvendo `pages=0` quando o índice sequer era usado (B-021)
6. O harness de upgrade declarando "todos passaram" com um cenário pulado (B-028)
7. O `theodb_hnsw` invisível ao diagnóstico por casar nome em vez de handler (B-021)

O caso do harness é o mais instrutivo: seu cabeçalho **documenta duas leituras falsas anteriores** e diz
que ele existe para que não haja uma terceira. Havia — e só apareceu porque o followup foi executado em
vez de registrado.

## Erros meus, registrados

- **B-020** — abri o item com um número **17× inflado** por contenção (60.643 ms → 3.524 ms isolado), e a
  razão de "93×" comparava estruturas diferentes. Segunda ocorrência do mesmo erro na sessão.
- **B-024** — afirmei no review do B-015 e no PR #226 que o autotune consumia os zeros. Não consumia.
- **Regressão de interpolação** — quebrei 3 testes com a forma exata que o arquivo já documenta (#172), e
  declarei o item completo validando com `cargo check --lib`, que não compila `#[cfg(test)]`.

## Quando destravar

O merge depende do critério "SOTA level" ser definido. Registro o que o próprio projeto já mediu:
`wiki/decisions/0035`/`0036` (M73/M74) declaram **não-alcançável** a superioridade de QPS vetorial sobre o
ScaNN/AlloyDB por uma extensão PostgreSQL permissiva — gap de paradigma, não de engenharia. Se o critério
incluir esse eixo, não fecha por construção; se for "paridade de recall + memória billion-scale +
AI-native/HTAP/aberto", o projeto já o declara alcançado, e o gate real passa a ser o dogfood (M141/M175).

**Custo de reter:** o `[Unreleased]` acumulava 120 entradas antes deste ciclo. Quanto maior o lote, mais
difícil isolar a causa quando algo quebrar no merge.
