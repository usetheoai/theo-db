---
slug: b044-stemming
items: [B-044]
date: 2026-08-13
branch: workspace
---

# Um analisador nomeado, para que índices antigos não precisem migrar

## Goal

Dar stemming e remoção de stopwords ao pilar lexical registrando um `TextAnalyzer` **sob nome próprio** —
nunca redefinindo `"default"` —, de modo que (a) `jumping` case `jumps`, (b) **todo índice já construído
continue respondendo exatamente como antes**, sem migração, (c) o erro de tokenizer desconhecido **falhe
alto** em vez de devolver vazio, e (d) o efeito em NDCG@10, recall@10 e MRR seja medido no mesmo caso do
`b040` e publicado mesmo se for negativo.

## Baseline Context

### Files that will be touched

| Arquivo | Estado | LoC | Papel |
|---|---|---|---|
| `theodb_rs/src/lexical/analyzer.rs` | **novo** | ~90 | o analisador nomeado: construção, registro e a constante do nome |
| `theodb_rs/src/lexical/engine.rs` | edição | ~+25/−6 | `build_schema` usa o nome; registro em `Index::create` e `Index::open`; erro de parse deixa de ser engolido |
| `theodb_rs/src/lexical/mod.rs` | edição | +1 | declara o módulo |
| `CHANGELOG.md` | edição | — | `[Unreleased]` |
| `wiki/benchmarks/b040-theodb-fts-msmarco.md` | **atualização** | — | os números novos ao lado dos antigos (ADR-0061: atualizar, não duplicar) |

`pg_backing.rs:82,125,158` e `lexical_core/src/probe.rs:74,117` montam schemas **de teste** e ficam como
estão — mudá-los faria os testes existentes medirem outra coisa sem que ninguém tivesse pedido.

### Current callers / dependents

| Símbolo | Onde | Papel |
|---|---|---|
| `build_schema()` | `engine.rs:36` | **único** ponto de escrita do schema de produção |
| `bm25_build` | `engine.rs:110` | `Index::create` + writer + flush + `bump_generation` |
| `bm25_search` | `engine.rs:156` | `read_generation` → cache → `QueryParser::for_index` |
| `open_from_heap` | `engine.rs:100` | `Index::open` — o ponto de abertura na **leitura** |
| `IndexCache` | `lexical_core/src/cache.rs:15` | chaveado por `(index_id, generation)`; rebuild invalida |
| `sanitize_query` | `engine.rs` | normaliza a consulta antes do parser; **não** stemiza, e não precisa |

### Domain glossary

| Termo | Significado aqui |
|---|---|
| **`TextAnalyzer`** | cadeia tokenizer + filtros do Tantivy, registrada por nome no `TokenizerManager` |
| **`TokenizerName`** | o nome serializado **no schema do campo**, dentro do `meta.json` do índice |
| **stemming (Snowball/Porter)** | `jumping` → `jump`. Aumenta recall, pode custar precisão |
| **stopword** | palavra funcional (`the`, `of`) removida do índice e da consulta |
| **`generation`** | contador em `theodb.lexical_index_meta`, bumpado a cada build; chave de invalidação do cache |
| **NDCG@10 / MRR** | métricas de qualidade de ranking com julgamento humano; a linha de base é 0,6962 / 0,667 |

### Architecture boundaries affected

```
bm25_build ──► build_schema() ──► "theodb_en" no schema ──► meta.json (serializado)
     │                                    ▲
     └──► Index::create ──► tokenizers().register("theodb_en", analyzer)
                                          │
bm25_search ──► open_from_heap ──► Index::open ──► tokenizers().register(...)
     └──► QueryParser::for_index ──► resolve pelo nome NO SCHEMA ──┘

índice antigo  ──► schema diz "default" ──► TokenizerManager padrão ──► comportamento intacto
```

A fronteira que importa: **o nome no schema é o contrato**. Ele decide qual analisador cada índice usa, e é
por isso que registrar um nome novo não toca em índice nenhum já existente.

## Prior Art

| Fonte | O que ensina | Onde |
|---|---|---|
| Oportunidade do B-044 | as medições que sustentam este plano | `.claude/knowledge-base/discoveries/opportunities/b044-stemming-opportunity.md` |
| `b040-theodb-fts-msmarco` | a linha de base a superar, e o handicap declarado | `wiki/benchmarks/b040-theodb-fts-msmarco.md` |
| `m186-lexical-ndcg-scifact-verdict` | nDCG 0,6269 no SciFact contra 0,3016 do `ts_rank_cd` | `wiki/benchmarks/m186-lexical-ndcg-scifact-verdict.md` |
| ADR-0061 | atualizar o artefato existente, não duplicar; publicar mesmo se piorar | `wiki/decisions/0061-benchmark-oficial-por-pilar.md` |
| Ciclo B-041 | erro engolido que vira zero linhas silencioso | `BACKLOG.md` |
| Parsimony ladder, degrau 4 | reusar dependência instalada em vez de escrever | `.claude/rules/parsimony-ladder.md` |

## Coverage Matrix

| # | Afirmação do Goal | Tarefa(s) |
|---|---|---|
| G1 | `jumping` casa `jumps`; stopwords removidas | T1.1, T1.2 |
| G2 | Índice antigo (schema `"default"`) responde como antes, sem migração | T1.3 |
| G3 | Tokenizer desconhecido **falha alto** em vez de devolver vazio | T1.4 |
| G4 | Efeito em NDCG/recall/MRR medido no mesmo caso do `b040` e publicado | T1.5 |

Cobertura: **4/4**.

## ADRs

### D1 — Nome novo (`theodb_en`), nunca redefinir `"default"`

**Decisão.** O analisador é registrado como `"theodb_en"`. `"default"` permanece o do Tantivy.

**Alternativas consideradas.**

1. *Redefinir `"default"` com a cadeia nova.* Menos código, um só nome. **Rejeitada, e é a rejeição central
   deste plano:** o nome do tokenizer é serializado no schema de cada índice
   (`text_options.rs:198-204`), então todo índice já construído diz `"default"`. Redefini-lo mudaria a
   semântica de busca de **toda instalação existente em silêncio** — consulta stemizada contra índice não
   stemizado, que degrada recall sem erro. É a classe de defeito que o [[B-041]] documenta.
2. *Nome novo + script de migração que reconstrói índices antigos.* **Rejeitada por YAGNI e por risco:**
   com nome novo não há nada a migrar — o índice antigo continua correto sob a própria semântica. Um script
   que reconstrói tudo introduz uma janela de falha para resolver um problema que não existe.

**Consequência.** Índices antigos ficam sem stemming até serem reconstruídos, e isso é **correto**: o
resultado deles continua consistente. Quem quiser stemming roda `bm25_build` de novo.

### D2 — Inglês fixo nesta versão, com o ponto de extensão explícito

**Decisão.** `Language::English`, constante, sem GUC e sem parâmetro em `bm25_build`.

**Alternativas consideradas.**

1. *GUC `theodb.lexical_language`.* **Rejeitada:** o idioma é propriedade **do índice**, não da sessão. Um
   GUC permitiria construir em inglês e consultar em francês — analisadores divergentes, recall degradado
   sem erro, exatamente o que a D1 evita. Se virasse GUC, precisaria ser gravado no schema de qualquer forma.
2. *Parâmetro em `bm25_build`.* É o desenho **certo a prazo** — o idioma pertence ao índice e o schema o
   carregaria. **Adiado por YAGNI medido:** os dois corpora que o projeto mede (MS MARCO, BEIR SciFact) são
   ingleses, não há usuário pedindo outro idioma, e mudar a assinatura pública de `bm25_build` é quebra que
   merece o seu próprio ciclo. O nome `theodb_en` **já reserva o espaço**: um `theodb_pt` futuro convive sem
   ambiguidade, e nenhum índice precisa mudar.

### D3 — Stopwords entram no mesmo analisador

**Decisão.** `StopWordFilter` para inglês entra na cadeia, junto com o stemmer.

**Alternativas consideradas.**

1. *Só stemmer nesta versão.* **Rejeitada:** as duas mudanças alteram a mesma cadeia e a mesma linha de base
   de medição. Separá-las obrigaria a duas corridas de benchmark (cada uma ~15 min e ~US$ 1) para medir um
   efeito que ninguém vai querer isolado — o handicap declarado no `b040` cita as duas juntas.
2. *Stopwords configuráveis.* YAGNI. Nenhum caso pede.

**Consequência aceita:** a remoção de stopwords muda o `fieldnorm` e portanto os scores absolutos de BM25.
Comparações com números anteriores só valem pelo ranking, não pelo score bruto — e o artefato dirá isso.

### D4 — O erro do parser deixa de ser engolido

**Decisão.** `qp.parse_query` com `Err` levanta `error!` em vez de devolver `TableIterator` vazio.

**Alternativas consideradas.**

1. *Manter o `Err(_) => vazio`.* **Rejeitada:** vazio é indistinguível de "nada casou". Um `UnknownTokenizer`
   — a falha exata que um registro malfeito produziria — passaria como resultado legítimo, e a corrida de
   benchmark reportaria NDCG 0 como medição.
2. *Logar e devolver vazio.* **Rejeitada:** log não chega a quem consulta; a Regra 8 pede falha alta.

**Consequência.** Consulta sintaticamente inválida passa a erro. **Risco real e mitigado:** `sanitize_query`
já reduz a consulta a alfanuméricos separados por espaço antes do parser, então uma consulta de usuário não
tem como produzir sintaxe inválida — o caminho só dispara em defeito de configuração. Um teste prova isso.

## Tasks

### T1.1 — O analisador nomeado existe e stemiza

#### Why this step

É a peça. Sem ela nada mais é verificável, e ela é pura — construir a cadeia não precisa de banco.

#### TDD

RED — em `analyzer.rs`, teste de unidade sem PostgreSQL:

```rust
#[test]
fn stems_english_inflections() {
    let mut a = build_analyzer();
    assert_eq!(tokens(&mut a, "jumping"), vec!["jump"]);
    assert_eq!(tokens(&mut a, "jumps"), vec!["jump"]);
}

#[test]
fn removes_english_stopwords() {
    let mut a = build_analyzer();
    assert_eq!(tokens(&mut a, "the lazy dog"), vec!["lazi", "dog"]);
}

#[test]
fn analyzer_name_is_not_the_tantivy_default() {
    assert_ne!(ANALYZER_NAME, "default");
}
```

GREEN — `SimpleTokenizer` → `RemoveLongFilter` → `LowerCaser` → `StopWordFilter::new(English)` →
`Stemmer::new(Language::English)`.

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `cargo test stems_english_inflections` sai 0 e `assert_eq!` casa `jumping` e `jumps` no mesmo radical
- `the` desaparece da saída do analisador — `assert_eq!(tokens(&mut a, "the lazy dog"), vec!["lazi","dog"])`
- `ANALYZER_NAME` **não** é `"default"` — `assert_ne!` explícito, porque redefinir o default é o defeito
  que a D1 existe para impedir

### T1.2 — O índice novo stemiza ponta a ponta, pelo SQL

#### Why this step

O analisador pode estar certo e não chegar ao índice: o registro precisa acontecer nos **dois** pontos de
abertura, e o schema precisa nomear a cadeia. Só o caminho SQL prova isso.

#### TDD

RED — `#[pg_test]`:

```rust
#[pg_test]
fn bm25_matches_across_inflection() {
    Spi::run("CREATE TABLE t(id bigint primary key, body text)").unwrap();
    Spi::run("INSERT INTO t VALUES (1,'the quick brown fox jumps over the lazy dog')").unwrap();
    Spi::run("SELECT bm25_build(700,'t','id','body')").unwrap();
    let hit = Spi::get_one::<i64>("SELECT id FROM bm25_search(700,'jumping',5)").unwrap();
    assert_eq!(hit, Some(1), "consulta flexionada não casou o documento");
}

#[pg_test]
fn stopword_only_query_returns_nothing() {
    // 'the' é stopword: some do índice e da consulta -> zero linhas, sem erro
    assert_eq!(count_of("SELECT count(*) FROM bm25_search(700,'the',5)"), 0);
}
```

GREEN — registro em `Index::create` (build) e em `open_from_heap` (busca), e `build_schema` nomeando a cadeia.

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `SELECT id FROM bm25_search(700,'jumping',5)` `returns` o documento 1 — hoje devolve vazio
- consulta só de stopword `returns` 0 linhas **sem erro** — é resultado legítimo, não falha
- a suíte `cargo pgrx test` completa continua verde: `equals` 0 falhas

### T1.3 — Índice antigo continua respondendo como antes

#### Why this step

É a afirmação mais forte deste plano e a que mais barato seria acreditar sem provar. A oportunidade a
sustenta por leitura da serialização do Tantivy; leitura não é medição.

#### TDD

RED — constrói um índice com o schema **antigo** (tokenizer `"default"`, montado explicitamente no teste) e
consulta com o binário novo:

```rust
#[pg_test]
fn legacy_index_keeps_default_analyzer_semantics() {
    build_with_legacy_default_schema(701, &["the quick brown fox jumps over the lazy dog"]);
    // sob "default" não há stemming: 'jumping' NÃO casa — e é o comportamento correto para este índice
    assert!(search(701, "jumping").is_empty());
    // e o que casava antes continua casando
    assert_eq!(search(701, "jumps"), vec![1]);
}
```

GREEN — nada a implementar se a D1 estiver certa; o teste é a prova. Se falhar, a D1 está errada e o plano
volta ao `/to-plan`.

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- índice com schema legado `returns` vazio para `jumping` e o documento para `jumps` — a semântica antiga,
  preservada
- o mesmo `index_id` reconstruído com `bm25_build` passa a stemizar — prova que o caminho de atualização é
  reconstruir, e que ele funciona
- nenhum código de migração foi escrito: `git diff` não acrescenta script de rebuild

### T1.4 — Tokenizer desconhecido falha alto

#### Why this step

O `Err(_) => vazio` de `engine.rs:188` transformaria um defeito de configuração em NDCG 0 silencioso — que
a corrida de benchmark publicaria como medição.

#### TDD

RED:

```rust
#[pg_test(error = "bm25: consulta inválida")]
fn parse_error_is_loud() { /* força um parse inválido */ }
```

GREEN — trocar o `Err(_) => return vazio` por `error!` com a consulta e o `index_id` na mensagem.

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- erro de parse levanta com mensagem `contains` o `index_id` e a consulta — exit não-zero, não lista vazia
- `test_sanitized_user_queries_never_reach_the_error_path` — parametrizado sobre `"lazy, DOG!"`, `"a+b"`, `"x AND -y"` e `"acentuação"`: cada um `returns` resultado ou lista vazia, **exit 0**, provando que `sanitize_query` protege o parser

### T1.5 — O efeito é medido no mesmo caso do b040 e publicado

#### Why this step

É o produto. E o ADR-0061 exige: mesma máquina, mesma corrida, qualidade ao lado de velocidade, artefato
atualizado em vez de duplicado.

#### TDD

RED — a corrida é o teste; a falsificação é a métrica ausente ou o artefato não atualizado:

```bash
CASE=FTSBm25Performance K=10 ./benchmarks/vectordbbench/run-fts.sh
jq -e '.results[0].metrics | .recall and .ndcg and .mrr' <json>
```

#### Concurrency tests

A corrida usa o `concurrent_runner` do arnês, que roda buscas em **parallel test** sobre o índice lexical
compartilhado — o mesmo caminho que o B-040 exercitou com até 80 clientes. Nenhuma mudança deste plano toca
a concorrência; o que se verifica é que ela não regrediu.

#### Acceptance criteria

- a corrida sai com `exit code 0` e o JSON contém `recall`, `ndcg` e `mrr` não nulos — `jq -e` sai 0
- `wiki/benchmarks/b040-theodb-fts-msmarco.md` é **atualizado** com os números novos ao lado dos antigos —
  `grep -c '0,6962'` continua `>= 1`, porque o número antigo não é apagado
- o custo de build é reportado antes e depois (linha de base ~2,5 s para 100K documentos)
- se NDCG cair, o artefato registra a queda: `grep -qE 'NDCG.*(caiu|regrediu|abaixo)' wiki/benchmarks/b040-theodb-fts-msmarco.md` sai 0, e o número novo `equals` o medido — nunca o antigo

## Failure scenarios

O pilar faz I/O: SPI para o heap, leitura do catálogo, e o benchmark fala PostgreSQL por rede.

| Cenário | Comportamento exigido | Onde |
|---|---|---|
| Analisador não registrado na abertura | `UnknownTokenizer` **falha alto**, não devolve vazio | T1.4 |
| Índice legado aberto pelo binário novo | usa `"default"`, comportamento intacto | T1.3 |
| `bm25_build` sobre tabela inexistente | erro do SPI propaga (comportamento atual, não alterado) | — |
| Consulta vazia após `sanitize_query` | devolve vazio sem erro — resultado legítimo | T1.4 |
| Corrida de benchmark falha | o gate do `run-fts.sh` recusa publicar | T1.5 |
| Stemmer muda scores absolutos | esperado; o artefato compara **ranking**, não score bruto | T1.5 |

## Concurrency tests

O índice lexical é compartilhado entre backends e o cache é por-backend, chaveado por
`(index_id, generation)`.

| Verificação | Como |
|---|---|
| Registro do analisador é idempotente entre backends | dois `Index::open` na mesma sessão não conflitam — o `TokenizerManager` é por-índice |
| Rebuild com analisador novo invalida o cache | `bump_generation` sobe; um **concurrent test** com duas sessões prova que a segunda vê o índice novo |
| A corrida do arnês a 80 clientes não regride | **parallel test** já exercitado no B-040; o QPS de pico é comparado com 1.616,4 |

## Dependencies

Nenhuma dependência nova. `tantivy = "0.26"` já está em `lexical_core/Cargo.toml:20` e traz `Stemmer`,
`StopWordFilter` e `Language`. Degrau 4 da parsimony ladder.

| Peça | Origem | Licença |
|---|---|---|
| `tantivy` 0.26.1 | já declarada | MIT — passa o gate D1 |
| `rust-stemmers` (transitiva do Tantivy) | já linkada | MIT/BSD-3 — passa o D1 |

## Drawbacks & Risks

| # | Risco | Probabilidade | Mitigação |
|---|---|---|---|
| R1 | **Stemming pode BAIXAR a precisão** — Porter é agressivo (`university`/`universe` colidem) | média | O NDCG é medido; se cair, publica-se assim mesmo (ADR-0061). É o resultado, não o fracasso |
| R2 | Remoção de stopwords muda `fieldnorm` e scores absolutos | certa | Declarado: a comparação é de ranking, não de score bruto |
| R3 | Índices antigos ficam sem stemming até rebuild | certa | É a D1, e é a escolha correta — o índice antigo continua **consistente** |
| R4 | Custo de build sobe (stemmer por token) | média | Medido em T1.5 contra a linha de base de ~2,5 s |
| R5 | Idioma fixo em inglês não serve corpus não-inglês | certa | Declarado na D2, com `theodb_en` reservando o espaço para `theodb_pt` |
| R6 | A D4 transforma parse inválido em erro; alguma consulta de usuário pode disparar | baixa | `sanitize_query` reduz a consulta a alfanuméricos antes do parser; teste prova |
| R7 | Ainda **sem significância pareada** ([[B-045]]) | certa | O delta de NDCG será **observado**, não demonstrado — e o artefato dirá isso |

## Unresolved Questions

- Q1 — **O stemmer melhora ou piora o NDCG neste corpus?** Não sei, e é o ponto da medição. MS MARCO é
  linguagem natural de consulta curta, onde stemming costuma ajudar recall; se a precisão cair mais, o
  resultado é negativo e vai publicado.
- Q2 — **Quanto o stemmer custa no build?** A linha de base é ~2,5 s para 100K documentos. Resolver por
  execução em T1.5.
- Q3 — **`StopWordFilter` do Tantivy usa qual lista para inglês?** Verificar na fonte antes de afirmar
  qualquer coisa sobre cobertura no artefato — não copiar a lista para o teste, apenas verificar que `the`
  desaparece.
