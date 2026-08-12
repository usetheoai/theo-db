---
slug: b034-guc-alias
items: [B-034]
date: 2026-08-12
branch: workspace
---

# Os GUCs de ajuste do pgvector passam a ter efeito

## Goal

Fazer `SET hnsw.ef_search` e `SET ivfflat.probes` — os nomes que toda aplicação pgvector emite para ajustar recall — terem o **mesmo efeito medido** que os nomes próprios do TheoDB, com precedência determinística e documentada, sem alterar o comportamento de quem já usa os nomes próprios.

## Baseline Context

**Base:** `0c42144` (workspace). Working tree limpa.

### Files that will be touched

| Arquivo | Linhas | Papel hoje | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/guc.rs` | ~520 | registra os GUCs de scan e expõe os pontos de leitura | **+2 registros, 2 pontos de leitura alterados, +2 statics** |
| `CHANGELOG.md` | — | contrato público | **+1 entrada** |

Nenhum arquivo removido. Nenhuma assinatura pública alterada — `guc::ef_search()` e `guc::probes()` mantêm tipo e nome.

### Current callers / dependents

| Chamador | Referência | Efeito |
|---|---|---|
| `am/scan.rs:299` | `guc::ef_search()` | passa a enxergar também o nome pgvector |
| `am/scan.rs:284` | `guc::probes()` | idem |
| `am/cost.rs:123` | ambos | idem |
| `am/customscan.rs:444` | `guc::probes()` | idem |
| Usuários de `theodb_hnsw.*` | — | **nenhum** — comportamento preservado |

### Domain glossary

| Termo | Significado neste plano |
|---|---|
| GUC | parâmetro de configuração do PostgreSQL, ajustável por sessão com `SET` |
| placeholder | GUC de prefixo não registrado; o PostgreSQL guarda o valor e ninguém o lê |
| alias | o nome pgvector (`hnsw.ef_search`), registrado para valer tanto quanto o próprio |
| precedência | qual nome vence quando os dois são setados na mesma sessão |
| ponto de leitura | a função de uma linha que o scan chama para obter o valor efetivo |

### Architecture boundaries affected

```
SET hnsw.ef_search      ──┐
                          ├──> guc::ef_search()  ──> am/scan.rs, am/cost.rs   [1 ponto]
SET theodb_hnsw.ef_search ┘

SET ivfflat.probes        ──┐
                            ├──> guc::probes()   ──> am/scan.rs, cost, customscan
SET theodb_ivfflat.probes ──┘
```

A fronteira nova é **aditiva e converge num ponto**: dois nomes, uma decisão, um valor efetivo. Nenhum consumidor muda.

## Prior Art

- **`theodb_rs/src/am/guc.rs:361,351`** — o padrão de registro que este plano estende. Os aliases usam a mesma chamada `GucRegistry::define_int_guc`, com as mesmas faixas mín/máx.
- **`pgrx-0.19.0/src/guc.rs:302,456`** — `define_int_guc` e `define_int_guc_with_hooks`. Medido: `GucSetting` **não** expõe a origem do valor, o que elimina "detectar se foi setado explicitamente" como estratégia.
- **`vector/vector--0.6.0.sql`** — o shim que já toma os nomes `hnsw` / `vector_l2_ops` para compatibilidade. Este plano completa a mesma política, um nível abaixo: o AM já tem o nome pgvector, o parâmetro de ajuste dele não tinha.
- **`.claude/rules/parsimony-ladder.md`** — degrau 5 (uma linha?): a precedência é uma comparação, não uma máquina de estados. O desenho com hooks foi rejeitado por isso (ADR D1).
- **`wiki/decisions/0029-m70-drop-pgvector.md` § D2** — a promessa de drop-in "sem mudança de código" que este item torna verdadeira mais uma camada.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa | Verificação |
|---|---|---|---|
| G1 | `hnsw.ef_search` tem efeito medido | T1.1 | recall difere entre dois valores do alias |
| G2 | `ivfflat.probes` tem efeito medido | T1.2 | idem para probes |
| G3 | Precedência determinística | T1.3 | teste com os dois setados, resultado previsto |
| G4 | Comportamento próprio preservado | T1.4 | `theodb_hnsw.ef_search` continua vencendo quando setado |
| G5 | GUCs visíveis em `pg_settings` | T1.5 | `count(*) WHERE name LIKE 'hnsw%'` deixa de ser 0 |
| G6 | Sem regressão nos consumidores | T1.6 | suíte completa verde |

Cobertura: **6 de 6 afirmações mapeadas (100%)**. Tarefas T1.1–T1.6, todas presentes.

## ADRs

### D1 — Precedência resolvida na LEITURA, não por hook de escrita

**Decisão:** os dois nomes têm armazenamento próprio e independente; o ponto de leitura decide. Regra: **o nome próprio vence quando está fora do default; caso contrário vale o alias.**

```
ef_search() = if EF_SEARCH != DEFAULT { EF_SEARCH } else { ALIAS_EF_SEARCH }
```

**Alternativas consideradas:**

1. *`assign_hook` no alias, espelhando no armazenamento próprio* (`define_int_guc_with_hooks` existe, medido). Semântica seria "o último `SET` vence", que é mais intuitiva. **Rejeitada por um risco de ordem:** o PostgreSQL restaura GUCs ao fim de transação e ao `RESET`, disparando os hooks — e a ordem entre duas variáveis independentes não é definida. Um rollback poderia deixar o valor efetivo vindo do alias restaurado depois do próprio. Trocar um defeito silencioso por outro, mais raro e mais difícil de diagnosticar, não é conserto.
2. *Detectar se o GUC foi setado explicitamente*, preferindo o específico. **Rejeitada por medição:** `GucSetting` do pgrx não expõe `source`; obter isso exigiria consultar `pg_settings` via SPI **dentro do caminho de scan**, o que é caro no lugar mais quente do produto.
3. *O maior valor vence.* **Rejeitada:** nunca reduziria recall por acidente, mas é regra que ninguém consegue explicar sem consultar a documentação, e viola a expectativa de que `SET` sobrescreve.
4. *Leitura com precedência do específico (escolhida).* Uma comparação, sem estado, sem hook, sem ordem para dar errado.

**Consequência aceita, e ela é a única aresta:** quem setar o nome **próprio exatamente ao valor default** e o alias a outro valor verá o alias vencer. É pathológico (setar ao default é o mesmo que não setar) e fica documentado.

**Por que o específico vence, e não o alias:** é a única ordem que não muda o comportamento de quem já usa o produto hoje. Um usuário atual que sete `theodb_hnsw.ef_search` continua obtendo exatamente o que obtinha.

### D2 — Tratar `ivfflat.probes` no mesmo ciclo, não depois

**Decisão:** os dois aliases entram juntos.

**Alternativas consideradas:**

1. *Só `hnsw.ef_search`, porque foi o medido.* **Rejeitada:** o defeito é idêntico em forma e causa — `theodb_ivfflat.probes` registrado, `ivfflat.probes` inerte. Entregar metade deixaria o usuário de IVFFlat com exatamente o mesmo silêncio, e o item voltaria.
2. *Ambos (escolhida).* O custo marginal é uma cópia de 15 linhas; a simetria dos pontos de leitura (`guc.rs:506` e `511`, ambos de uma linha) torna a segunda metade quase gratuita.

### D3 — Registrar como GUC de verdade, aceitando a quebra do placeholder

**Decisão:** registrar `hnsw.ef_search` e `ivfflat.probes` com as mesmas faixas mín/máx dos próprios.

**Consequência declarada:** hoje `SET hnsw.ef_search = 99999` é aceito em silêncio (placeholder sem validação). Depois da mudança, o PostgreSQL valida na conversão do placeholder e **erra**. É comportamento correto — o valor sempre foi inválido —, mas é mudança visível e vai ao CHANGELOG.

**Alternativa considerada:** faixas mais largas no alias para não quebrar ninguém. **Rejeitada:** aceitar valor que o motor não honra é o mesmo defeito de outra forma.

## Tasks

### T1.1 — `hnsw.ef_search` passa a ter efeito

#### Why this step

É o defeito medido no B-034 e o que bloqueia o B-035. Sem ele, qualquer varredura de `ef_search` por ferramenta externa produz curva plana.

#### TDD

```
test_pgvector_ef_search_alias_has_effect
  arrange: tabela com índice theodb_hnsw e dados suficientes para ef importar
  act:     SET hnsw.ef_search = 1   -> medir recall
           SET hnsw.ef_search = 200 -> medir recall
  assert:  recall(200) > recall(1)
  estado hoje: FALHA — os dois recalls são iguais, porque o GUC é inerte
```

Medir **recall**, não aceitação do `SET`: o teste existe justamente porque o `SET` já é aceito hoje.

#### Acceptance criteria

- `cargo pgrx test pg18 -- ef_search_alias` termina em exit code 0
- o recall difere entre os dois valores

### T1.2 — `ivfflat.probes` passa a ter efeito

#### Why this step

Mesmo defeito, mesma causa, outro índice. Entregar só o hnsw deixaria o usuário de IVFFlat com o mesmo silêncio.

#### TDD

```
test_pgvector_probes_alias_has_effect
  arrange: tabela com índice theodb_ivfflat
  act:     SET ivfflat.probes = 1 e depois = lists
  assert:  recall cresce com probes
  estado hoje: FALHA — inerte
```

#### Acceptance criteria

- `cargo pgrx test pg18 -- probes_alias` termina em exit code 0
- o valor efetivo de `guc::probes()` equals o valor setado por `SET ivfflat.probes`

### T1.3 — A precedência é determinística

#### Why this step

É a única decisão de desenho do item, e uma regra não testada é uma regra que diverge da documentação na primeira mudança.

#### TDD

```
test_alias_precedence_specific_wins
  act:    SET theodb_hnsw.ef_search = 200; SET hnsw.ef_search = 1;
  assert: o valor efetivo é 200 — o específico vence quando fora do default
  estado hoje: FALHA (a função não existe)
```

#### Acceptance criteria

- `cargo pgrx test pg18 -- alias_precedence` termina em exit code 0
- com `theodb_hnsw.ef_search = 300` e `hnsw.ef_search = 7`, o efetivo returns 300
- o comentário de `resolve_alias` enuncia a regra na mesma linguagem do ADR D1

### T1.4 — O comportamento existente não muda

#### Why this step

Rede de regressão. O risco de um alias é sequestrar o caminho que já funcionava.

#### TDD

```
test_native_guc_unchanged
  act:    SET theodb_hnsw.ef_search = 300 (sem tocar no alias)
  assert: valor efetivo 300
  estado hoje: PASSA — é rede, não alvo
```

#### Acceptance criteria

- `cargo pgrx test pg18 -- native_guc_unchanged` termina em exit code 0 antes E depois da mudança
- `SET theodb_hnsw.ef_search = 123` produz efetivo que equals 123

### T1.5 — Os GUCs ficam visíveis no catálogo

#### Why this step

Hoje `pg_settings` não os mostra, e é assim que um operador descobre que um parâmetro existe. Visibilidade é parte da correção, não enfeite.

#### TDD

```
test_alias_gucs_are_registered
  act:    SELECT count(*) FROM pg_settings WHERE name IN ('hnsw.ef_search','ivfflat.probes')
  assert: 2
  estado hoje: FALHA (devolve 0)
```

#### Acceptance criteria

- `SELECT count(*) FROM pg_settings WHERE name IN ('hnsw.ef_search','ivfflat.probes')` returns 2
- o mesmo comando returns 0 antes da mudança, medido

### T1.6 — Sem regressão

#### Why this step

Os pontos de leitura alterados são consumidos pelo scan e pelo modelo de custo — o caminho mais quente do produto.

#### TDD

```
suíte completa: cargo pgrx test pg18
assert: 451 testes seguem verdes, mais os novos
```

#### Acceptance criteria

- `cargo pgrx test pg18 --no-default-features --features "pg18 pg_test"` termina em exit code 0
- a contagem de testes verdes é >= 451, o baseline medido no ciclo anterior

## Failure scenarios

O caminho não faz I/O externo; os cenários são de configuração.

| Cenário | Comportamento exigido | Onde é provado |
|---|---|---|
| Só o alias setado | vale o alias | T1.1, T1.2 |
| Só o próprio setado | vale o próprio | T1.4 |
| Ambos setados | vence o próprio | T1.3 |
| Nenhum setado | vale o default (64 / 1) | T1.4 (implícito no baseline) |
| Alias fora de faixa | erro do PostgreSQL na conversão do placeholder — hoje passa em silêncio | declarado no ADR D3 e no CHANGELOG |
| `RESET` do alias | volta ao default; o próprio, se setado, continua vencendo | consequência direta do desenho de leitura (ADR D1) |

## Concurrency tests

**(none — single-threaded.)** GUCs são estado por sessão (backend), não compartilhado entre processos. A mudança acrescenta leitura de uma variável de sessão a mais no mesmo ponto onde já se lia outra. Nenhum estado novo é compartilhado, nenhum caminho concorrente é criado.

## Dependencies

**Nenhuma dependência nova.** Degrau 4 da parsimony ladder.

| Dependência | Versão | Já instalada | Papel |
|---|---|---|---|
| `pgrx` | 0.19.0 | sim | `GucRegistry::define_int_guc` |
| `cargo-pgrx` | 0.19.0 | sim | `cargo pgrx test pg18` |
| PostgreSQL | 18.4 | sim | alvo do teste |

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação |
|---|---|---|---|
| R1 | Um alias sequestrar o caminho que já funciona, mudando comportamento de usuário atual | alta | ADR D1 escolhe precedência do específico exatamente por isso; T1.4 é a rede |
| R2 | Quebra de sessões que hoje setam valor fora de faixa em silêncio | média | declarado no ADR D3 e no CHANGELOG; o valor sempre foi inválido, apenas não era validado |
| R3 | Aresta de precedência: próprio setado ao default + alias setado | baixa | documentada no ADR D1; setar ao default é equivalente a não setar |
| R4 | Testar "o `SET` foi aceito" em vez de "o valor teve efeito" — o defeito é justamente o `SET` ser aceito | alta | os testes medem **recall**, não aceitação; está escrito no TDD de T1.1 e T1.2 |
| R5 | O CI está vermelho (B-029), então a validação disponível é local | média | suíte no contêiner com toolchain pinado, como nos ciclos anteriores |

## Unresolved Questions

- Q1: Há outros GUCs do pgvector que aplicações emitem e que o TheoDB ignora em silêncio? A medição deste item cobriu `ef_search` e `probes`. Levantar o inventário completo do upstream contra o nosso é trabalho de outro item, e vira B-036 se houver.
- Q2: O `hnsw.iterative_scan` (pgvector 0.8) tem equivalente aqui (`theodb_hnsw.resume`)? Não medido. Mesma pergunta do Q1, mesmo encaminhamento.
