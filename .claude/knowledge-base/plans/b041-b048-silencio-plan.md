---
slug: b041-b048-silencio
items: [B-041, B-048]
date: 2026-08-13
upstream: .claude/knowledge-base/discoveries/opportunities/b041-b048-silencio-opportunity.md
---

# Plano — o pilar lexical para de responder onde deveria recusar

## Goal

Fazer o `bm25_search` **recusar** o índice que nunca foi construído, e o `bm25_build` **contar apenas o que é
achável** — sem quebrar o caso legítimo de corpus vazio, que é o que separa este conserto de uma regressão.

O item [[B-048]] nomeia uma **classe** que o projeto já consertou três vezes e que reapareceu três vezes. Este
plano conserta as instâncias vivas e **não** tenta erradicar a classe: mecanizá-la é item próprio, e prometer
erradicação num plano de duas funções seria a promessa que a própria classe já quebrou seis vezes.

## Baseline Context

### O estado medido

| Fato | Medido em `theodb:b036` |
|---|---|
| `bm25_search(999,…)` sobre índice nunca construído | **0 linhas, sem erro** |
| `bm25_build` sobre 3 linhas com um `body` NULL | devolve **3**; só ids **1 e 3** são acháveis |
| `bm25_build` sobre corpus **vazio** | devolve 0 e **registra** `generation 1` no catálogo |
| `bm25_build` sobre corpus com 1 doc | devolve 1 e registra `generation 1` |
| `theodb.lexical_index_meta` | tem linha por build, **inclusive o vazio** |

A última linha é a que decide o desenho: o catálogo significa *"um build aconteceu"*, não *"existem
documentos"*.

### Files that will be touched

| Arquivo | LoC | Papel |
|---|---|---|
| `theodb_rs/src/lexical/engine.rs` | ~420 | `read_generation` (`:105-110`), `bm25_build` (`:150-163`), `bm25_search` (`:177-195`) |
| `CHANGELOG.md` | — | um erro novo onde havia silêncio é mudança observável |

Estado do git: `5bf7484` em `workspace`.

### Current callers / dependents

- `bm25_search` é `#[pg_extern]` — superfície SQL pública, sem chamador interno em Rust
- O **cliente do VectorDBBench** (`theodb.py:330`, fork) já faz este guard por conta própria, consultando
  `lexical_index_meta`. Ele continua funcionando: um erro do servidor é o que ele já espera detectar antes
- As corridas publicadas (b040, b044, b047) **sempre** construíram antes de buscar — nenhum número muda
- `theo-rag` e `theo-memory` **não usam** o pilar lexical hoje (usam pgvector), então o blast radius de produto
  é zero neste momento

### Architecture boundaries affected

- `rules/error-handling.md § 2`: "NUNCA engula exceções" e "retorne erros explícitos em vez de valores mágicos"
  — o `0` de `read_generation` é literalmente o valor mágico que a regra proíbe
- `rules/architecture.md § 2`: validação na fronteira. `bm25_search` **é** a fronteira (superfície SQL)
- Nenhuma camada nova; nenhuma dependência nova

### Domain glossary

- **generation** — contador incrementado a cada `bm25_build` de um `index_id`. `≥ 1` significa que houve build.
- **índice não construído** — `index_id` sem linha em `theodb.lexical_index_meta`. Hoje indistinguível, para
  quem chama, de um corpus onde nada casou.
- **documento achável** — documento cujo `body` não é NULL e portanto pode casar um termo.

## Prior Art

- **[[B-034]]** — GUC aceito em silêncio sem efeito. Mesma classe, consertada com erro tipado.
- **[[B-044]]** — o `bm25_search` engolia erro do parser de consulta; consertado com `error!`. **Mesma função.**
- **[[B-041]]** e o cliente do [[B-040]] — o cliente teve de consultar o catálogo por conta própria para
  conseguir falhar alto, o que é a evidência de que a informação está no lugar certo e a função no errado.
- **`explain_scan`/`scan_stats`** — zeros silenciosos consertados com erro tipado, e a justificativa escrita lá
  vale palavra por palavra aqui.

## Drawbacks & Risks

| # | Risco | Prob. | Mitigação |
|---|---|---|---|
| R1 | O guard quebra o caso legítimo de corpus vazio | **alta se feito por contagem** | O guard consulta **presença no catálogo**, não contagem de documentos — e foi medido que o build vazio registra `generation 1`. O teste do caso vazio é o gate |
| R2 | Um erro novo onde havia silêncio quebra consumidor existente | baixa — nenhum consumidor de produto usa o pilar | Vai para o CHANGELOG como mudança observável; o cliente do arnês já falhava antes por conta própria |
| R3 | O custo do guard entra no caminho quente | média | Já existe uma consulta a `lexical_index_meta` por busca (`read_generation`); o conserto **não acrescenta** consulta, só deixa de engolir o resultado |
| R4 | Pular o NULL muda o conteúdo do índice | baixa | Um documento de body vazio nunca casou termo nenhum — para a busca, presente-mas-inerte e ausente são o mesmo. Só o **contador** muda, e é o contador que estava errado |
| R5 | "Consertar a classe" vira escopo infinito | **alta se prometido** | Declarado fora: este plano conserta as instâncias vivas; mecanizar a classe é item próprio |

## Unresolved Questions

- Q1 — Existem outros `#[pg_extern]` do pilar lexical devolvendo valor mágico? A T1.4 varre e **conta**, em vez
  de supor que são só estes.
- Q2 — O `generation` pode ser 0 para um índice de fato construído, em alguma versão antiga do catálogo? Não
  medi contra base antiga; a T1.1 trata `0` e ausência da mesma forma, o que é seguro nos dois sentidos.

## ADRs

### D1 — O guard consulta PRESENÇA no catálogo, nunca contagem de documentos

**Decisão.** `read_generation` passa a devolver `Option<u64>`: `None` quando não há linha, `Some(g)` quando há.
`bm25_search` levanta erro tipado em `None` e prossegue em `Some(_)`, **inclusive quando o corpus está vazio**.

**Alternativas consideradas.**

- *Errar quando a busca devolve zero resultados.* Rejeitada, e é o erro que o plano existe para evitar: zero
  resultados é resposta legítima. Confundir os dois trocaria um falso-silêncio por um falso-alarme.
- *Errar quando o índice tem zero documentos.* Rejeitada pela medição: o build vazio é estado válido e registra
  `generation 1`. Um usuário que constrói sobre tabela vazia e depois insere está no caminho correto.
- *Deixar o guard no cliente.* Rejeitada: é onde ele está hoje, e por isso protege **as nossas corridas** e não
  quem instala o produto.

**Custo aceito.** Nenhuma consulta a mais: `read_generation` já é chamada por busca. O que muda é o que se faz
com o resultado.

### D2 — `read_generation` para de transformar erro em `0`

**Decisão.** A cadeia `.ok()`/`.unwrap_or(0)` sai. Erro do SPI **propaga**; ausência de linha vira `None`.

**Razão.** `rules/error-handling.md § 2` proíbe valor mágico para sinalizar falha, e `§ 5` chama de
anti-pattern exatamente "retornar `null` em vez de lançar quando a operação falhou". Hoje um catálogo
inacessível e um índice não construído produzem o mesmo `0`, e o chamador não tem como distinguir.

**Alternativas consideradas.**

- *Logar o erro e continuar com 0.* Rejeitada: log não é contrato, e o `rules/error-handling.md § 5` lista
  "logar e engolir" como anti-pattern nomeado.
- *`panic!` no erro de SPI.* Rejeitada: sob pgrx um panic atravessa a fronteira C; o caminho correto é o
  `error!` do pgrx, que vira `ereport(ERROR)` com SQLSTATE.

### D3 — `bm25_build` conta o que indexa; o NULL não é indexado

**Decisão.** Linhas com `body` NULL são **puladas** e não entram no contador.

**Alternativas consideradas.**

- *Indexar como string vazia e não contar.* Rejeitada: deixa lixo no índice que nunca casa, e o custo de
  armazená-lo é real num corpus grande com muitos NULLs.
- *Erguer erro no NULL.* Rejeitada: um corpus com `body` NULL é dado legítimo — o `bm25_build` não é quem
  decide o esquema do usuário. Ele deve **relatar honestamente** quantos indexou.
- *Filtrar no SQL (`WHERE body IS NOT NULL`).* **Considerada e preferível** se o `SELECT` for nosso: menos
  linhas trafegadas pelo SPI. A T1.3 decide lendo se o `SELECT` é construído aqui.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa |
|---|---|---|
| C1 | `bm25_search` recusa índice não construído | T1.1 |
| C2 | O caso legítimo de corpus vazio continua devolvendo zero sem erro | T1.2 |
| C3 | `bm25_build` conta só o achável | T1.3 |
| C4 | `read_generation` distingue "sem build" de "consulta falhou" | T1.1 (D2) |
| C5 | A classe é medida, não erradicada por decreto | T1.4 |

## Tasks

### T1.1 — `bm25_search` recusa, e `read_generation` para de mentir

#### Why this step

É o defeito do [[B-041]] e o (c) do [[B-048]] no mesmo ponto: os dois nascem de `0` significar duas coisas.

#### TDD

RED:

```rust
#[pg_test(error = "bm25_search: index_id 999 nunca foi construído")]
fn search_on_never_built_index_raises_typed_error() {
    Spi::get_one::<i64>("SELECT count(*) FROM bm25_search(999, 'lazy dog', 5)").unwrap();
}
```

GREEN — `read_generation` devolve `Option<u64>`; `bm25_search` levanta em `None`.

#### Concurrency tests

(none — single-threaded) A leitura do catálogo roda no snapshot do backend; o pgrx executa `#[pg_test]`
sequencialmente num backend só. Concorrência entre build e busca é o gate MVCC do M139, já coberto.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Catálogo inacessível (SPI falha) | erro **propaga** — nunca vira "índice não construído" |
| `index_id` negativo | mesma recusa: não está no catálogo |
| Linha presente com `generation` 0 | tratado como ausente — seguro nos dois sentidos (Q2) |

#### Acceptance criteria

- `SELECT count(*) FROM bm25_search(999,'x',5)` **levanta** erro cuja mensagem **contains** `999` e a frase
  `nunca foi construído`
- `grep -c "unwrap_or(0)" theodb_rs/src/lexical/engine.rs` `equals` 0
- a suíte Rust fecha com `0 failed` e o total **>= 478**

### T1.2 — O corpus vazio continua sendo resposta, não erro

#### Why this step

É o gate que impede o conserto de virar regressão, e o risco R1 é alto justamente porque a implementação
ingênua (contar documentos) quebraria aqui.

#### TDD

RED:

```rust
#[pg_test]
fn search_on_built_but_empty_index_returns_zero_rows_without_error() {
    Spi::run("CREATE TABLE vazio(id bigint PRIMARY KEY, body text)").unwrap();
    Spi::get_one::<i64>("SELECT bm25_build(4242,'vazio','id','body')").unwrap();
    let n = Spi::get_one::<i64>("SELECT count(*) FROM bm25_search(4242,'qualquer',5)").unwrap();
    assert_eq!(n, Some(0), "corpus vazio é resultado legítimo, não erro");
}
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `search_on_built_but_empty_index_returns_zero_rows_without_error` **returns** `0 failed`, e a asserção `assert_eq!(n, Some(0))` **falharia** se o guard contasse documentos — verificado invertendo o guard uma vez e revertendo
- `bm25_build` sobre tabela vazia **returns** `0` e registra linha no catálogo — verificado por
  `SELECT count(*) FROM theodb.lexical_index_meta WHERE index_id = 4242` `equals` 1

### T1.3 — `bm25_build` conta o que é achável

#### Why this step

O retorno é a única informação que o usuário tem sobre o que foi indexado. Contar 3 quando 2 são acháveis é a
classe do [[B-034]] com outra roupa: o valor devolvido não descreve o que aconteceu.

#### TDD

RED:

```rust
#[pg_test]
fn build_counts_only_findable_documents() {
    Spi::run("CREATE TABLE d(id bigint PRIMARY KEY, body text)").unwrap();
    Spi::run("INSERT INTO d VALUES (1,'alpha'),(2,NULL),(3,'beta')").unwrap();
    let n = Spi::get_one::<i64>("SELECT bm25_build(4243,'d','id','body')").unwrap();
    assert_eq!(n, Some(2), "o NULL não é achável e não deve ser contado");
    // e o contador BATE com o que a busca encontra — a asserção que impede contar por outro caminho errado
    let achaveis = Spi::get_one::<i64>(
        "SELECT count(*) FROM bm25_search(4243,'alpha OR beta',10)").unwrap();
    assert_eq!(achaveis, Some(2));
}
```

#### Concurrency tests

(none — single-threaded)

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Todos os `body` NULL | devolve 0 **e registra** no catálogo — é build válido de corpus sem documento achável |
| `body` string vazia (não NULL) | conta? **decidir e testar**: string vazia é dado do usuário, NULL é ausência. Trato como NULL — não é achável |

#### Acceptance criteria

- o retorno de `bm25_build` **equals** o número de documentos que a busca subsequente encontra, no corpus do
  teste
- tabela com todos os `body` NULL: retorno `0` e linha no catálogo presente

### T1.4 — A classe é MEDIDA, não erradicada por decreto

#### Why this step

O [[B-048]] existe porque a classe voltou seis vezes. Consertar três instâncias e declarar a classe resolvida
seria a sétima promessa. O que cabe aqui é **contar** as instâncias restantes, para que o próximo item nasça de
um número.

#### TDD

Varredura estrutural sobre a superfície SQL do pilar:

```bash
grep -nE "unwrap_or\(0\)|unwrap_or_default\(\)|\.ok\(\)" theodb_rs/src/lexical/*.rs
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- a contagem de valores-mágicos remanescentes em `lexical/` é **publicada no review**, com `arquivo:linha`
- cada remanescente ganha uma das três etiquetas literais `corrigido`, `legitimo` ou `item-novo` na tabela do review — `grep -c` das três na seção `equals` o número de linhas da tabela
- `grep -cE "unwrap_or\(0\)|unwrap_or_default\(\)" theodb_rs/src/lexical/` **equals** o número de linhas etiquetadas `legitimo` mais as etiquetadas `item-novo` — se sobrar uma sem etiqueta, os dois números divergem

## Failure scenarios

Consolidados por tarefa. O caminho não tem I/O externo — é SPI e heap dentro do próprio backend. A classe de
falha que já custou caro neste projeto está coberta na T1.2: **o conserto do silêncio virando falso-alarme**.

## Definition of done

- [ ] `bm25_search` sobre índice não construído levanta erro tipado nomeando o `index_id`
- [ ] corpus vazio construído continua devolvendo zero linhas sem erro — teste que falharia com a implementação ingênua
- [ ] `bm25_build` devolve o número de documentos acháveis, verificado contra a busca
- [ ] `read_generation` não engole erro de SPI; `unwrap_or(0)` some do arquivo
- [ ] os valores-mágicos remanescentes em `lexical/` são contados e classificados no review
- [ ] suíte Rust `>= 478 passed; 0 failed`
- [ ] CHANGELOG registra a mudança observável de comportamento
