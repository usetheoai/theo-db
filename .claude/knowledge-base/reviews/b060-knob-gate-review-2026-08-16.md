---
slug: b060-knob-gate
item: B-060
repo: theodb-bench
date: 2026-08-16
base: b6a5bfd
head: 6cdc1bd
verdict: READY_TO_MERGE
---

# Review — a tarefa que media o instrumento derrubou o instrumento que o plano tinha escolhido

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte | **637 passed; 0 failed** (era 627 — 10 novos) |
| 2 | `mypy --strict` | **limpo**, 36 arquivos |
| 3 | `ruff check src/ tests/` | **All checks passed** |
| 4 | Prova por reprovação | o teste do gate **falha** contra o código anterior, verificado com `git stash` |
| 5 | Segredos commitados | **0** |
| 6 | Idioma do repo | inglês em código, docstrings, CHANGELOG e commit — per `CLAUDE.md` do `theodb-bench` |
| 7 | `CHANGELOG.md` atualizado | sim — 3 entradas em `Added` |
| 8 | Schemas versionados | **nenhum bump** — e a razão está registrada (§ R-3) |

## Cross-validation — 5 de 5

| # | Afirmação do Goal | Como foi verificada | Resultado |
|---|---|---|---|
| C1 | Um parâmetro pedido e não vigente **recusa** a medição | `test_gate_refuses_an_adapter_that_accepts_the_knob_and_ignores_it` | passa; **falha** contra o código anterior |
| C2 | O efetivo é lido do servidor | `SELECT setting, source FROM pg_settings WHERE name = %s` | lido, não inferido |
| C3 | Vale para **todo** adapter | `test_every_adapter_reports_effective_search_parameters` parametrizado sobre `ADAPTERS` | 4 de 4 |
| C4 | O gate é provado reprovando | `git stash` + `pytest -k refuses_an_adapter_that_accepts` | `1 failed` sem o conserto |
| C5 | O bundle distingue pedido de efetivo | `test_the_bundle_records_what_was_in_force_not_only_what_was_asked` | passa, e **sem** bump de schema |

## Achados

### R-1 — ALTO · A T1.1 existia para medir o instrumento, e foi ela que salvou o item

A D2 do plano dizia, em texto, que o efetivo seria lido com `current_setting`. A T1.1 mandava **medir o
comportamento do placeholder antes de escrever o gate**. Medido, em `postgres:18-bookworm` puro:

```
SET nao.existe = 999;                                     → SET   (sucede)
SELECT current_setting('nao.existe', true);                → 999   (ecoa o que eu escrevi)
SELECT count(*) FROM pg_settings WHERE name='nao.existe';  → 0
```

**Um gate sobre `current_setting` pediria 200, leria 200, e mediria o default.** Falso-negativo perfeito para
exatamente o defeito que ele existe para pegar — e eu teria escrito esse gate se a tarefa não exigisse medir
primeiro.

`pg_settings` é a autoridade porque lista **apenas GUC registrado**.

### R-2 — ALTO · A mesma armadilha do concorrente existe no TheoDB, e agora está medida

A terceira consulta da T1.1, contra o produto:

```
theodb:b036, sessão nova
  SELECT count(*) FROM pg_settings WHERE name LIKE 'theodb%';   →  0
  LOAD 'theodb_rs';
  SELECT count(*) FROM pg_settings WHERE name LIKE 'theodb%';   → 38
  SET theodb_hnsw.ef_search = 200;                              → setting=200 source=session
```

Os 38 GUCs só existem **depois** de a biblioteca carregar na sessão. Antes disso,
`SET theodb_hnsw.ef_search` é placeholder e não faz nada.

**É a mesma condição que faz o `scann.num_leaves_to_search` do AlloyDB falhar em silêncio sem
`LOAD 'alloydb_scann'`** — a armadilha que custou uma corrida de 10 milhões de vetores ao avaliador
independente. Ele a encontrou no concorrente; ela está aqui.

Isto não é defeito do TheoDB (é como o PostgreSQL registra GUC de extensão), e é precisamente por isso que o
gate tem de existir no arnês: o motor está correto, e a corrida é que pode medir o ponto errado.

### R-3 — MÉDIO · O bundle publicava um ponto de operação que nunca existiu, e o conserto não custou schema

`src/bench/vector.py:334` monta `PointResult(parameters={**index.parameters, **search})` — do **pedido** —,
e o `set_search_parameters` só roda na linha 347. Para o `probes`, que é clampado ao número de listas, um pedido
de `10000` numa tabela de 10k linhas é enviado como o clamp: **o artefato diria 10000, e 50 estaria em vigor.**

O efetivo passou a entrar no mesmo dicionário, **chaveado pelo nome do GUC** (`ivfflat.probes`), o que o torna
distinguível da chave lógica (`probes`) e dispensa bump: `points[].parameters` já é declarado como objeto aberto
de escalares no schema, então isto usa o campo como ele foi projetado em vez de acrescentar propriedade a schema
versionado.

**A D3 previa a escolha e a medição a resolveu.** Sem medir a linha 334, eu teria concluído que o efetivo era
redundante — afinal, o gate garante que pedido e vigente coincidem, ou não há corrida. O clamp é a exceção que
desfaz esse raciocínio.

### R-4 — MÉDIO · O `FakeAdapter` participa de verdade, e a razão é onde ele roda

O primeiro `pytest` deixou 8 dos 9 verdes: só `[fake]` reprovava. Era o R4 do plano acontecendo — e a tentação
era um stub que devolvesse o pedido.

Ele passou a devolver o que aplicou de verdade, com a razão no docstring: sendo in-process, efetivo **é** o
pedido; e ele é o duplo que os testes do runner exercitam, então um contrato que o dispensasse ficaria sem
cobertura no caminho que mais roda.

### R-5 — BAIXO · Um `noqa` que o linter recusou, e a recusa estava certa

Escrevi `# noqa: BLE001` no `except Exception`. O `ruff` reprovou com `RUF100 Unused noqa directive
(non-enabled: BLE001)` — a regra não está habilitada neste repo, então o `noqa` era ruído silenciando nada.

Removi em vez de habilitar a regra ou suprimir o `RUF100`. Vale o registro porque a alternativa —
`# noqa: RUF100` sobre o `# noqa: BLE001` — é como uma supressão vira duas.

### R-6 — INFORMATIVO · Uma afirmação do plano que a implementação não precisou

A D2 previa normalizar a comparação por tipo (`int`, `bool`, `enum`), citando o risco R1 de reprovar por
`"64" != 64`. Não foi necessário: o mapa declara o literal **como string** (`str(int(value))`), e `pg_settings`
devolve string. A comparação é `str == str`, sem conversão.

O risco R1 continua real para um motor futuro cujo GUC seja booleano ou enum — o ScaNN traz
`quantizer='sq8'`. Registrado aqui em vez de implementado: o degrau 1 da parsimony ladder recusa a normalização
que nenhum knob atual exige, e o [[B-059]] a trará se o ScaNN precisar.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **O gate não foi exercitado contra um servidor real.** Os testes usam duplos que respondem `pg_settings`; a
  medição do comportamento real do PostgreSQL foi feita à mão (T1.1) e está registrada, mas **não** há teste de
  integração que rode o gate contra um contêiner. É a afirmação mais fraca deste ciclo.
- **`source` pode ter outros valores** além de `session` e `default` (`configuration file`, `command line`).
  O gate recusa apenas `default`; um GUC vindo do `postgresql.conf` com o valor correto passa, o que é
  desejável, mas não foi testado.
- **A normalização por tipo não existe** (§ R-6) — declarada, não implementada.

## Veredito

**`READY_TO_MERGE`.**

5 de 5 afirmações verificadas por execução; 637 testes; mypy strict e ruff limpos; o gate provado reprovando o
código anterior.

**Ressalvas:** review do próprio implementador; o gate não tem teste de integração contra servidor real; e a
normalização por tipo fica para quando um knob a exigir.
