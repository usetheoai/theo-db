---
slug: b036-build-reloptions
items: [B-036]
date: 2026-08-13
branch: workspace
---

# Ligar a ponta que falta: o build lê `m` e `ef_construction` da relação

## Goal

Fazer `CREATE INDEX ... USING hnsw (...) WITH (m=N, ef_construction=N)` funcionar e **ser honrado**, de modo
que (a) os dois valores sejam reloptions de verdade com faixa validada, (b) o build inicial e o fold do
VACUUM os leiam da relação em vez das constantes, (c) índices já criados continuem abrindo e varrendo com o
mesmo recall de hoje, e (d) a variável de ambiente redundante desapareça — duas fontes para o mesmo knob é a
armadilha de precedência que o [[B-034]] pagou para resolver.

## Baseline Context

### Files that will be touched

| Arquivo | Estado | LoC | Papel |
|---|---|---|---|
| `theodb_rs/src/am/options.rs` | edição | ~+70 | 2 campos no struct, 2 `add_int_reloption`, 2 entradas na tabela de parse, 2 acessores |
| `theodb_rs/src/am/build.rs` | edição | ~+15/−8 | os dois call sites leem da relação; a env var sai |
| `CHANGELOG.md` | edição | — | `[Unreleased]` |
| `wiki/decisions/` | — | — | nenhum ADR novo: a decisão de desenho está aqui e é reversível |

**Não tocados, e isso é o achado:** `am/hnsw_page/meta.rs`, `search.rs`, `store.rs`, `scan.rs`. A varredura já
lê `meta.m`.

### Current callers / dependents

| Símbolo | Onde | Papel |
|---|---|---|
| `HNSW_M = 16` | `build.rs:22` | constante consumida nos dois call sites de build |
| `HNSW_EF_CONSTRUCTION = 64` | `build.rs:23` | idem, via `hnsw_ef_construction()` |
| `hnsw_ef_construction()` | `build.rs:30-36` | lê `THEODB_HNSW_EF_CONSTRUCTION` (env do servidor) |
| build inicial | `build.rs:416` | `HnswIndex::build_owned(corpus, HNSW_M, hnsw_ef_construction(), …)`; tem `indexrel` |
| fold do VACUUM | `build.rs:706`, dentro de `vacuum_rebuild_hnsw_structured(indexrel, …)` (`:676`) | reconstrói o grafo **inteiro** a partir de `live`; **também tem `indexrel`** |
| `meta.m` / `meta.m0` | `hnsw_page/meta.rs:40` | **persistidos**, e lidos por `search.rs:186,379,454`, `store.rs:297,377`, `scan.rs:513` |
| `lists_from_relation` e irmãs | `options.rs:314+` | o padrão de acessor a copiar |
| `TheodbIvfflatOptions` | `options.rs:88` | struct de reloptions compartilhada pelos dois AMs |

### Domain glossary

| Termo | Significado aqui |
|---|---|
| **`m`** | grau máximo de vizinhos por nó no HNSW acima do nível 0; `m0 = 2m` no nível 0 |
| **`ef_construction`** | tamanho da lista de candidatos durante a **construção**. Sobe qualidade do grafo, sobe custo de build |
| **reloption** | opção de índice em `WITH (…)`, gravada como texto em `pg_class.reloptions` e parseada por `amoptions` em `rd_options` |
| **fold do VACUUM** | compactação que **reconstrói o grafo inteiro** a partir dos vetores vivos |
| **`rd_options`** | o bytea parseado que o relcache mantém; nulo quando o índice não declarou opção nenhuma |
| **meta de página** | bloco 0 do índice; carrega `m`, `m0`, métrica, e os descritores de quantização |

### Architecture boundaries affected

```
CREATE INDEX WITH (m, ef_construction)
      │
      ▼  amoptions ──► rd_options (TheodbIvfflatOptions)
      │
build.rs:416 ──► m_from_relation / ef_construction_from_relation ──► HnswIndex::build_owned
      │                                                                      │
      └──────────────────────────────────────────────────────► meta.m gravado no bloco 0
                                                                             │
VACUUM fold (build.rs:706, tem indexrel) ──► mesmos acessores ──► reconstrói o grafo inteiro
                                                                             │
                                          scan/search/store ◄────── já leem meta.m (INTOCADOS)
```

A fronteira que importa: **a leitura já respeita o índice**. Este plano só liga a escrita.

## Prior Art

| Fonte | O que ensina | Onde |
|---|---|---|
| Oportunidade do B-036 | as medições que sustentam este plano | `.claude/knowledge-base/discoveries/opportunities/b036-build-reloptions-opportunity.md` |
| `lists_from_relation` e as 9 irmãs | o padrão exato de acessor: `rd_options` nulo → default; fora de faixa → default | `am/options.rs:314-470` |
| Docstring de `sbq_bits_from_relation` | *"A fold reads this off the persisted meta (not the reloption), so this is only the initial-build gate"* — e **por que aqui é diferente** (D2) | `am/options.rs` |
| B-026 (remoção do `degree_bound`) | reloption registrada só na tabela de parse e não no `add_int_reloption` é **rejeitada** pelo PG antes do parse | `BACKLOG.md` |
| Ciclo B-034 | duas fontes para o mesmo knob é armadilha de precedência | `.claude/knowledge-base/releases/b034-release.md` |
| `b035-theodb-vs-pgvector-pg18` | os defaults 16/64 coincidem com os do pgvector; mudá-los invalidaria as corridas publicadas | `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` |

## Coverage Matrix

| # | Afirmação do Goal | Tarefa(s) |
|---|---|---|
| G1 | `m` e `ef_construction` são reloptions com faixa validada | T1.1 |
| G2 | O build inicial e o fold os honram, lidos da relação | T1.2, T1.3 |
| G3 | Índices existentes continuam abrindo e varrendo igual | T1.4 |
| G4 | A env var redundante desaparece | T1.5 |

Cobertura: **4/4**.

## ADRs

### D1 — Defaults permanecem `m=16` / `ef_construction=64`

**Decisão.** Os defaults das reloptions novas são exatamente as constantes de hoje.

**Alternativas consideradas.**

1. *Adotar defaults "melhores" agora que são ajustáveis.* **Rejeitada:** os valores atuais coincidem com os
   do pgvector, e é isso que torna maçã-com-maçã toda corrida já publicada (`b035`, `b040`, `b044`, `b047`).
   Mudar o default invalidaria quatro artefatos de uma vez, e o momento de escolher um default melhor é
   **depois** de o [[B-042]] medir qual é — que é o que este item destrava.

### D2 — O fold lê a RELOPTION, não o meta persistido — e por que aqui isso é correto

**Decisão.** `vacuum_rebuild_hnsw_structured` usa os mesmos acessores de relação que o build inicial.

**Alternativas consideradas.**

1. *Ler do meta persistido, como o `sbq_bits` faz.* É o padrão estabelecido no mesmo arquivo, e a alternativa
   que eu esperava adotar. **Rejeitada depois de medir a diferença:** o `sbq_bits` precisa vir do meta porque
   determina o **codebook**, que já está gravado nas páginas — ler o reloption ali produziria códigos
   incompatíveis com os dados. O fold do HNSW é outra coisa: ele **reconstrói o grafo inteiro** a partir dos
   vetores vivos (`build.rs:697-709`), então qualquer `m` produz um grafo novo, completo e autoconsistente,
   cujo `meta.m` é regravado junto. Não há mismatch possível.
2. *Persistir `ef_construction` no meta e ler de lá.* **Rejeitada por YAGNI medido:** exigiria bump de versão
   do meta (hoje em v4) e mexer no encode/decode, para resolver um mismatch que a alternativa 1 mostra não
   existir. O degrau 1 da parsimony ladder: a persistência não precisa existir.

**Consequência, dita porque é comportamento observável:** depois de `ALTER INDEX … SET (m=32)`, o índice
continua com `m=16` até a próxima reconstrução (fold de VACUUM ou `REINDEX`), quando passa a 32. **É
exatamente a semântica que o PostgreSQL dá às próprias reloptions** — `fillfactor` funciona assim —, não uma
surpresa nossa.

### D3 — `THEODB_HNSW_EF_CONSTRUCTION` é removida

**Decisão.** A variável de ambiente sai junto com `hnsw_ef_construction()`.

**Alternativas consideradas.**

1. *Manter a env var como override.* **Rejeitada:** duas fontes para o mesmo knob obrigam uma regra de
   precedência, e o [[B-034]] custou um ciclo inteiro para resolver exatamente isso num par de GUCs. A env
   var existia porque **não havia reloption** — o docstring dela diz "benchmark-only knob"; com reloption de
   verdade ela perde a razão de ser.
2. *Manter e documentar a precedência.* **Rejeitada:** documentar uma armadilha não é o mesmo que removê-la.

### D4 — Faixas validadas pelo próprio `build_reloptions`

**Decisão.** `MIN/MAX` passados ao `add_int_reloption`; valor fora de faixa é rejeitado pelo PostgreSQL no
`CREATE INDEX`.

**Alternativas consideradas.**

1. *Clampar no acessor, como `lists_from_relation` faz (fora de faixa → default).* **Rejeitada para a
   entrada:** clampar em silêncio é a classe do [[B-048]] — o usuário pede `m=1000`, recebe 16 e não sabe.
   O `build_reloptions` já rejeita fora de faixa com erro nomeado quando `validate` está ligado; o clamp do
   acessor permanece como rede de segurança para dado já gravado, não como política de entrada.

## Tasks

### T1.1 — As duas reloptions existem e a faixa é validada

#### Why this step

Sem o `add_int_reloption` o PostgreSQL rejeita a opção antes do parse — foi o que o B-026 mediu ao remover o
`degree_bound`. É a fundação e é onde a validação de faixa mora.

#### TDD

RED — `#[pg_test]`:

```rust
#[pg_test]
fn create_index_accepts_m_and_ef_construction() {
    Spi::run("CREATE TABLE t036(id int, e vector(8))").unwrap();
    Spi::run("INSERT INTO t036 SELECT g, array_fill(random()::real, ARRAY[8])::vector FROM generate_series(1,200) g").unwrap();
    Spi::run("CREATE INDEX i036 ON t036 USING hnsw (e vector_l2_ops) WITH (m=32, ef_construction=200)").unwrap();
}

#[pg_test(error = "value 999999 out of bounds for option \"m\"")]
fn m_out_of_range_is_refused_not_clamped() { /* CREATE INDEX ... WITH (m=999999) */ }
```

GREEN — 2 campos no struct (ao final, como o B-026 documenta), 2 `add_int_reloption`, 2 entradas na tabela de
parse (que passa de 9 para 11).

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `CREATE INDEX ... WITH (m=32, ef_construction=200)` sai com sucesso — hoje dá `unrecognized parameter "m"`
- valor fora de faixa **levanta** com mensagem que nomeia a opção: `assert` sobre `out of bounds for option`
- a tabela de parse tem **11** entradas e o array é declarado `[…; 11]` — o descasamento é erro de compilação,
  não de runtime

### T1.2 — O build inicial honra os valores

#### Why this step

Aceitar a opção e ignorá-la é literalmente o defeito do B-034 numa camada nova. O teste tem de medir
**efeito**, não aceitação.

#### TDD

RED:

```rust
#[pg_test]
fn build_honors_m_from_relation() {
    // dois índices, mesmo corpus, m diferente -> meta.m diferente, lido de volta do disco
    build_with(801, "m=16"); build_with(802, "m=32");
    assert_eq!(meta_m_of("i036_a"), 16);
    assert_eq!(meta_m_of("i036_b"), 32);
}

#[pg_test]
fn build_honors_ef_construction_by_recall() {
    // ef_construction baixo vs alto no mesmo corpus -> recall MEDIDO diferente
    let low  = recall_of_index_built_with("ef_construction=16");
    let high = recall_of_index_built_with("ef_construction=400");
    assert!(high > low, "ef_construction alto deve dar recall >= baixo, {high} vs {low}");
}
```

GREEN — `m_from_relation` / `ef_construction_from_relation` no padrão de `lists_from_relation`, e o call site
de `build.rs:416` passa a usá-los.

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `meta.m` lido do disco `equals` o valor pedido, para dois valores distintos
- recall com `ef_construction=400` é **maior** que com **`4`** no mesmo corpus (o plano dizia `16`) — asserção
  sobre recall medido, nunca sobre o `CREATE INDEX` ter sido aceito.

  **Substituição registrada, com a razão (2026-08-13).** O par `16 → 400` não é seguro como asserção: o próprio
  projeto mediu no M57 que subir `efc` de 64 para 200 **piorou** o recall a 100k–500k (`build.rs:15-21`), e
  codificar "maior sempre" seria afirmar em teste uma monotonicidade que a nossa própria medição refutou. Com
  `efc=4` num corpus de 300 nós a busca gulosa do build é pobre demais para empatar, e o teste
  `ef_construction_changes_the_measured_recall_end_to_end` **mediu** `high > low` — efeito real, não predição.

  Dois testes complementares fecham o elo que o recall sozinho não fecharia, porque `ef_construction` não é
  persistido em lugar nenhum: `ef_construction_reloption_reaches_the_accessor` (SQL → acessor, determinístico) e
  `the_builder_actually_consumes_ef_construction` (o builder produz grafos diferentes para `efc` diferente, com
  o controle de determinismo ao lado — sem ele, a diferença poderia ser ruído em vez de efeito do parâmetro)
- índice criado **sem** as opções produz `meta.m` `equals` 16, o default de hoje

### T1.3 — O fold do VACUUM honra os mesmos valores

#### Why this step

É onde o desenho podia dar errado em silêncio: um VACUUM que reconstrói com parâmetros diferentes dos de
criação muda o índice sem ninguém pedir. A D2 argumenta que ler o reloption é correto **porque o fold
reconstrói tudo** — e argumento não é medição.

#### TDD

RED:

```rust
#[pg_test]
fn fold_rebuild_preserves_m_from_the_reloption() {
    build_with(810, "m=32");
    delete_half_the_rows();
    Spi::run("VACUUM t036").unwrap();          // dispara o fold
    assert_eq!(meta_m_of("i036_c"), 32, "o fold não pode reconstruir com o default");
    assert!(recall_after_vacuum() > 0.5, "o índice segue utilizável após o fold");
}
```

GREEN — o call site de `build.rs:706` usa os mesmos acessores; `indexrel` já está em escopo (`:676`).

#### Concurrency tests

O fold roda dentro de um `VACUUM`, que toma seus próprios locks; este plano não altera o protocolo de lock.
A verificação é que o **parallel test** existente do build (`pg_hnsw_parallel_build_recall_reasonable`) segue
verde, provando que ler da relação não mudou o caminho paralelo.

#### Acceptance criteria

- `meta.m` **após** o VACUUM `equals` o valor de criação, não o default
- o índice segue varrendo após o fold: recall `> 0.5` no mesmo corpus
- `pg_hnsw_parallel_build_recall_reasonable` continua verde — `equals` 0 falhas

### T1.4 — Índices existentes não mudam

#### Why this step

É a afirmação que mais barato seria acreditar. A oportunidade a sustenta porque `rd_options` é nulo quando
nenhuma opção foi declarada — e nulo é o caminho que devolve o default.

#### TDD

RED:

```rust
#[pg_test]
fn index_without_options_behaves_exactly_as_before() {
    Spi::run("CREATE INDEX i_legacy ON t036 USING hnsw (e vector_l2_ops)").unwrap();  // sem WITH
    assert_eq!(meta_m_of("i_legacy"), 16);
    assert!(recall_of("i_legacy") >= RECALL_BASELINE);
}
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- índice sem `WITH` tem `meta.m` `equals` 16 e recall `>=` o de hoje
- a suíte completa continua verde: `cargo pgrx test pg18` reporta **0 failed**, e o total **não diminui**

### T1.5 — A env var redundante desaparece

#### Why this step

Duas fontes para o mesmo knob obrigam uma regra de precedência que ninguém pediu, e o B-034 mostrou o custo
de descobrir isso tarde.

#### TDD

RED — verificação sobre a fonte:

```bash
grep -c "THEODB_HNSW_EF_CONSTRUCTION" theodb_rs/src/    # deve dar 0
grep -c "fn hnsw_ef_construction" theodb_rs/src/        # deve dar 0
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- ~~`grep -c "THEODB_HNSW_EF_CONSTRUCTION" theodb_rs/src/` `equals` 0~~ — **corrigido por acréscimo na
  implementação (2026-08-13).** O comentário que EXPLICA a remoção cita o nome da variável, então satisfazer a
  letra deste critério significaria apagar a explicação. É a mesma armadilha que o T1.1 do B-045 pagou. O que o
  critério quis dizer é "nenhuma **dependência**", e dependência aqui é a leitura: o `grep` correto é
  `grep -c "env::var(\"THEODB_HNSW_EF_CONSTRUCTION" theodb_rs/src/`, que dá **0**
- `THEODB_HNSW_PARALLEL_THRESHOLD` **permanece** — é knob de bissecção de contenção, não de qualidade, e não
  tem reloption equivalente. Removê-la seria escopo que ninguém pediu
- o CHANGELOG registra a remoção sob `Changed`, porque quem usava a env var em benchmark precisa saber

### T1.6 — Os dois call sites que a descoberta não enumerou (acrescentado na implementação)

#### Why this step

A varredura de consumidores antes de editar encontrou **dois** usos das constantes fora dos dois call sites
mapeados. Nenhum dos dois aparece na oportunidade, e ignorá-los deixaria o item entregue pela metade:

| Call site | O que quebraria |
|---|---|
| `hnsw_page/store.rs:383` — caminho de **INSERT** | um índice criado com `ef_construction=200` voltaria a 64 a cada linha inserida depois do build, em silêncio (o `efc` não está no meta, então nada denunciaria) |
| `cost.rs:123` — **estimativa de custo** do planner | o custo seguiria descrevendo um grafo `m=16` para um índice `m=32`; o comentário do arquivo afirmava textualmente que `m` era constante |
| `build.rs:843` — `ambuildempty_hnsw` | o índice **vazio** grava um meta, e é ele que o primeiro INSERT lê; nascer com o default divergiria do pedido |

Os três leem a reloption da relação, não o meta — no `cost.rs` isso é obrigatório e não preferência: uma
segunda leitura de página dentro do `amcostestimate` seria uma superfície de `Err` nova num caminho que **não
pode** abortar plano nenhum (contrato EC-3, documentado no próprio arquivo).

#### Acceptance criteria

- `grep -c "crate::am::build::HNSW_EF_CONSTRUCTION" theodb_rs/src/am/hnsw_page/store.rs` `equals` 0
- `cost.rs` não importa mais `HNSW_M` — `grep -c "use crate::am::build::HNSW_M" theodb_rs/src/am/cost.rs` `equals` 0
- a suíte completa segue verde com o total **maior** que o baseline de 469

## Failure scenarios

O caminho é in-process (relcache + build), sem I/O externo além do próprio storage do índice.

| Cenário | Comportamento exigido | Onde |
|---|---|---|
| `rd_options` nulo (índice sem opções) | devolve o default; **não** desreferencia | T1.4 |
| Valor fora de faixa no `CREATE INDEX` | **rejeitado** pelo PostgreSQL, nomeando a opção | T1.1 |
| Valor fora de faixa já gravado (índice antigo, faixa mudada) | acessor clampa para o default — rede de segurança, não política de entrada | T1.1 (D4) |
| `ALTER INDEX SET (m=…)` sem rebuild | índice mantém o `m` de criação até o próximo fold/REINDEX | D2, declarado |
| Fold sobre índice sem opções | reconstrói com os defaults, como hoje | T1.3 |

## Concurrency tests

O build HNSW usa `std::thread::scope` acima de 4.096 nós (`ann/hnsw_parallel.rs:18`).

| Verificação | Como |
|---|---|
| Ler `m` da relação não quebra o caminho paralelo | o **parallel test** `pg_hnsw_parallel_build_recall_reasonable` segue verde |
| O acessor é chamado uma vez, fora do laço | inspeção: `m_from_relation` é avaliado antes do `build_owned`, não por nó |

## Dependencies

Nenhuma dependência nova. Tudo é `pg_sys` já em uso.

## Drawbacks & Risks

| # | Risco | Probabilidade | Mitigação |
|---|---|---|---|
| R1 | `ALTER INDEX SET` sem rebuild confunde quem espera efeito imediato | média | É a semântica do PostgreSQL para reloptions; declarada na D2 e no CHANGELOG |
| R2 | Acrescentar campo ao struct desloca offsets | baixa | Campos vão **ao final**, e `rd_options` é reconstruído a cada relcache load — não é formato persistido |
| R3 | Alguém usava `THEODB_HNSW_EF_CONSTRUCTION` em benchmark | baixa | Registrado em `Changed`; a reloption cobre o caso e melhor (é por índice, não por servidor) |
| R4 | `m` grande estoura o limite de página do HNSW | média | A faixa máxima é escolhida a partir do `nbr_size(level) <= BLCKSZ` que o `ann/hnsw.rs:75` já documenta, e T1.1 prova a recusa |
| R5 | O ganho de destravar o B-042/B-046 pode não existir | certa até medir | Este item **não** promete ganho: promete o experimento. O ganho é o que o B-042 vai medir |

## Unresolved Questions

- Q1 — **Qual `m` máximo cabe numa página?** `ann/hnsw.rs:75` diz que `nbr_size(level) ≤ BLCKSZ` limita, e cita
  `m=16` a 1M dando ~5 níveis. Resolver por leitura do cálculo em T1.1, e escolher `MAX_M` a partir dele em
  vez de um número redondo.
- Q2 — **`m0` continua sendo `2m` ou vira ajustável?** Hoje é derivado. Mantê-lo derivado é o mínimo que
  resolve; expor um terceiro knob é YAGNI até alguém pedir.
- Q3 — **RESOLVIDA por leitura, e a conclusão muda o escopo.** Há um único `RELOPT_KIND` e um único
  `amoptions` compartilhado pelos dois AMs (`am/mod.rs:156`; `options.rs:83` diz "shared by the two AMs").
  Logo o `theodb_ivfflat` **já aceita hoje** `sbq_bits`, `pq_subspaces` e as outras seis opções que são
  exclusivas do HNSW, em silêncio e sem efeito. Acrescentar `m`/`ef_construction` estende uma condição
  pré-existente em vez de criar uma nova.

  **Fica FORA deste item, e registrado:** separar as opções por AM é trabalho próprio, da família do
  [[B-048]] (a superfície aceita onde deveria recusar), e misturá-lo aqui faria um item que fecha nunca.
