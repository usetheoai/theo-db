---
slug: b041-b048-silencio
items: [B-041, B-048]
date: 2026-08-13
base: 5bf7484
head: ab941db
verdict: READY_TO_MERGE
---

# Review — um teste pré-existente afirmava o defeito, e era o mais bem-nomeado da suíte

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte Rust | **480 passed; 0 failed** (477 + 4 novos − 1 duplicata removida) |
| 2 | `/code-quality` | **`PASS_WITH_CAVEATS` (89)**, **0 achados HARD** |
| 3 | Compilação com o gate certo | `cargo check --features pg18,pg_test --all-targets` → exit 0 (lição R-8 do ciclo anterior, aplicada) |
| 4 | Segredos commitados | **0** |
| 5 | Commit direto em `main` | não — `workspace` |
| 6 | Trailer de coautoria | **0** |
| 7 | `CHANGELOG.md` atualizado | sim — 3 entradas em `Fixed`, incluindo a mudança observável |

## Cross-validation — 5 de 5

| # | Afirmação do Goal | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | `bm25_search` recusa índice não construído | `search_on_never_built_index_raises_typed_error`, `#[pg_test(error = …)]` com a mensagem completa | passa — e o pgrx compara por igualdade, então passar **é** a prova de que a mensagem casa inteira |
| G2 | O caso legítimo de corpus vazio não quebrou | `search_on_built_but_empty_index_returns_zero_rows_without_error` | passa: build devolve `0`, **registra** no catálogo, busca devolve `0` **sem erro** |
| G3 | `bm25_build` conta só o achável | `build_counts_only_findable_documents` compara o retorno com o que a busca encontra | `Some(2)` nos dois — retorno e realidade batem |
| G4 | Corpus todo-NULL é build válido | `build_over_all_null_bodies_is_a_valid_empty_build` | `0` documentos, linha no catálogo presente, busca sem erro |
| G5 | `read_generation` não engole erro | `grep` por `unwrap_or(0)` em código executável | **0** ocorrências |

## Achados

### R-1 — ALTO · Um teste pré-existente codificava o defeito como contrato

`test_bm25_search_empty_index_returns_no_rows` afirmava, literalmente:

```rust
assert_eq!(n, Some(0), "index with no build -> 0 rows, not an error");
```

sobre o `index_id` 999, que nunca passou por `bm25_build`. **A suíte estava protegendo o defeito.**

E o que o escondeu foi o nome. Ele diz *"empty index"*; a montagem usa um índice **nunca construído**. São
estados diferentes — um é resposta legítima, o outro é a aplicação não saber que esqueceu o build —, e o nome
fazia o primeiro cobrir o segundo. Qualquer pessoa lendo a lista de testes veria "índice vazio devolve zero
linhas" e concordaria, sem perceber que a montagem falava de outra coisa.

**Removido, não afrouxado.** Nenhuma cobertura se perdeu: os dois estados ganharam teste próprio, cada um
afirmando o que o seu nome diz. `rules/testing.md § 3` pede que o nome descreva o comportamento — aqui ele
descrevia um comportamento e o corpo verificava outro.

### R-2 — ALTO · O guard consulta PRESENÇA, e a medição que decidiu isso quase saiu errada

A implementação ingênua — errar quando o índice tem zero documentos — quebraria o caso legítimo. A medição que
descartou essa via:

```
SELECT bm25_build(100,'cheio','id','body');   -- 1 documento  → 1
SELECT bm25_build(200,'vazio','id','body');   -- 0 documentos → 0

 index_id | generation
      100 |         1
      200 |         1
```

**O build vazio registra.** Então `lexical_index_meta` significa *"um build aconteceu"*, não *"existem
documentos"* — exatamente a semântica de que o guard precisa.

**E eu quase publiquei o contrário.** Numa primeira leitura concluí que o catálogo *não* distinguia os dois
casos, e que consertar exigiria mudar o caminho de `bm25_build`. Estava errado: eu havia lido
`lexical_index_meta` **antes** de executar o segundo build. Re-medido em contêiner limpo, com os dois builds na
mesma transação, o registro está lá. **A ordem em que eu li produziu um defeito que não existe**, e publicá-lo
teria levado a uma mudança desnecessária num caminho quente.

### R-3 — MÉDIO · O escopo do B-048 encolheu por medição, e a parte que saiu não era de produto

O item listava três instâncias. Medidas:

| | Estado |
|---|---|
| **(a)** `bm25_build` conta NULL | **confirmado por execução** e corrigido |
| **(b)** `pg_backing.rs:201` devolve 0 | **FORA do binário default** — vive em `#[cfg(feature = "spike-lexical")]`, declarado no arquivo como *"andaime de medicao"*. O caminho equivalente que **está** no default (`open_from_heap`) já falha alto |
| **(c)** `read_generation` engole erro | **achado de leitura**, não de execução — reproduzi-lo exigiria fazer o SPI falhar de propósito. Corrigido mesmo assim, porque a regra que ele viola é textual |

Registrar que (b) não chega ao usuário **não é minimizar o item** — é o que impede o próximo leitor de gastar
tempo num caminho que o produto não expõe.

### R-4 — MÉDIO · A classe foi CONTADA, não declarada resolvida

O [[B-048]] existe porque a classe "responder onde deveria recusar" já foi consertada três vezes e voltou
outras três. Consertar mais três instâncias e declarar a classe morta seria a sétima promessa.

A T1.4 varreu o pilar por valores-mágicos em **código executável** (comentários excluídos — as três primeiras
ocorrências que o `grep` devolveu eram as minhas próprias explicações, a mesma armadilha que o B-045 e o B-036
já pagaram). Resta **uma**:

| Ocorrência | Classificação |
|---|---|
| `engine.rs:103` — `unwrap_or(false)` sobre `to_regclass(...) IS NOT NULL` | **`legitimo`** — ausência do catálogo significa que nenhum build jamais aconteceu, e `false` mapeia corretamente para `None` |

Zero remanescentes sem classificação. Mecanizar a classe continua sendo item próprio.

### R-5 — INFORMATIVO · O ciclo de 8 minutos, e o que ele custou a este item

O owner interrompeu o trabalho dizendo *"7 min executando e nada? isso é impossível de trabalhar"*, e estava
certo. Decomposto:

| Custo | Tempo | Evitável |
|---|---|---|
| Recompilar **363 crates** | 2m34s | **sim** — `cp -r` não preserva mtime; com `cp -a`: **109** |
| Rodar os **480** `pg_test` | 5m24s | não — cada um sobe SQL contra um Postgres real |
| Rodar os 480 para validar **um módulo** | — | **sim** — `cargo pgrx test pg18 lexical` filtra |

Registrado como [[B-054]]. Vale como achado deste review porque um ciclo lento **empurra para o gate errado**:
foi exatamente ele que me fez usar `cargo check` sem `pg_test` no ciclo anterior e descobrir o erro 25 minutos
depois (R-8).

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **O custo do guard não foi medido em produção.** O plano afirma que não há consulta a mais — `read_generation`
  já era chamada por busca —, e isso é verdade por leitura do diff, não por medição de latência.
- **`read_generation` com SPI falhando não foi exercitado.** O erro agora propaga por construção
  (`.expect()` sob pgrx vira `ereport`), mas não existe teste que force a falha do SPI.
- **A mudança de comportamento não foi validada contra consumidor real.** Nenhum produto do time usa o pilar
  lexical hoje (usam pgvector), então o blast radius é zero **neste momento** — e essa é uma propriedade da data,
  não do desenho.
- **O CI segue sem rodar sobre `workspace`** ([[B-052]]).

## Veredito

**`READY_TO_MERGE`.**

5 de 5 afirmações verificadas por execução; 480 testes verdes; `/code-quality` `PASS_WITH_CAVEATS` com 0
achados HARD; a classe do B-048 contada e classificada em vez de declarada resolvida.

**Ressalvas:** review do próprio implementador; o custo do guard é argumentado, não medido; e o item entrega o
conserto de três instâncias de uma classe que já voltou seis vezes — mecanizá-la continua aberto.
