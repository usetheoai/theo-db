---
item: B-044
mode: evolve
date: 2026-08-13
verdict: pending
---

# B-044 — O stemmer já está linkado; o que falta é nomeá-lo, e o risco de migração some por construção

## Corner 1 — Evidence

Medido em 2026-08-12/13 contra `theodb:b034` em execução e contra o código-fonte de
`tantivy-0.26.1` vendorizado em `~/.cargo/registry`.

### O sintoma, e o que de fato falta

| Consulta | Resultado | Leitura |
|---|---|---|
| `lazy dog` | `1:1,767 4:1,691` | multi-termo OR com scores somados — **BM25 correto** |
| `what does the lazy dog do all day` | `4:5,543 1:2,978` | pergunta natural rankeia o certo em primeiro |
| **`jumping`** (corpus tem `jumps`) | **vazio** | **sem stemming — é este item** |
| `"frase exata"` | idêntico ao sem aspas | sem semântica de frase |
| `lazy AND dog` | traz também o doc que contém "and" | `AND` é termo, não operador |
| `jump*` | vazio | sem curinga |
| `the` | devolve documentos | stopwords indexadas, não removidas |

### A causa, localizada

`theodb_rs/src/lexical/engine.rs:36-42` monta o schema de produção:

```rust
fn build_schema() -> (Schema, Field, Field) {
    let mut sb = Schema::builder();
    let id = sb.add_i64_field("id", STORED | FAST | INDEXED);
    let body = sb.add_text_field("body", TEXT | STORED);   // ← aqui
    ...
}
```

O `TEXT` do Tantivy mapeia para o tokenizer `"default"`
(`tantivy-0.26.1/src/schema/field_entry.rs:160`), que é SimpleTokenizer + RemoveLong + LowerCaser —
**sem stemmer e sem stopwords**. É um único ponto de escrita; `pg_backing.rs:82,125,158` e `probe.rs:74,117`
são schemas de **teste**, não o de produção.

### O que o Tantivy 0.26.1 já traz — degrau 4 da parsimony ladder

| Peça | Onde |
|---|---|
| `Stemmer`, `StemmerFilter`, `Language` | `src/tokenizer/stemmer.rs:12,63,95` |
| `StopWordFilter` | `src/tokenizer/stop_word_filter/` |
| `LowerCaser`, `RemoveLongFilter`, `SimpleTokenizer` | `src/tokenizer/` |

**Não se escreve um stemmer. Configura-se o que já está linkado.**

### O fato que decide o desenho: o nome do tokenizer é serializado POR CAMPO

`TextFieldIndexing` (`src/schema/text_options.rs:198-204`) tem `#[serde(default)] tokenizer: TokenizerName`,
e o schema vai para o `meta.json` do índice. Consequência medida:

- Um índice construído **antes** desta mudança carrega `"default"` no próprio schema, **para sempre**.
- `"default"` continua registrado pelo `TokenizerManager` padrão do Tantivy.
- Logo, **um índice antigo lido pelo binário novo usa `"default"` e se comporta exatamente como antes**.

**O risco de migração que o item previa desaparece por construção** — desde que o analisador novo seja
**registrado sob um nome novo** em vez de redefinir `"default"`. Redefinir `"default"` seria o oposto: mudaria
a semântica de todo índice existente em silêncio.

### O caminho de consulta já herda o analisador de graça

`bm25_search` usa `QueryParser::for_index(index, vec![body_f])` (`engine.rs:187`). O parser resolve o
tokenizer **pelo schema do campo** e o busca no `TokenizerManager` do índice — se não achar, devolve
`QueryParserError::UnknownTokenizer` (`query_parser.rs:88`). Ou seja: indexação e consulta usam o mesmo
analisador **automaticamente**, sem código de sincronização, contanto que o registro exista nos dois pontos
de abertura (`Index::create` no build, `Index::open` na busca).

`sanitize_query` (`engine.rs`) normaliza a consulta antes do parser — minúsculas e corte em não-alfanuméricos.
Ele **não** stemiza e não precisa: o stemming vem pelo tokenizer do campo. Também é por isso que operadores
não funcionam hoje — são removidos antes de o parser os ver. **Operadores estão fora do escopo deste item**
por decisão registrada: são superfície de consulta, não analisador.

### Um defeito encontrado de passagem

`engine.rs:188-191` engole o erro do parser:

```rust
let parsed = match qp.parse_query(&clean) {
    Ok(q) => q,
    Err(_) => return TableIterator::new(Vec::new().into_iter()),
};
```

Um `UnknownTokenizer` — exatamente a falha que uma migração malfeita produziria — devolveria **zero linhas
sem erro**, indistinguível de "nada casou". É a mesma classe do [[B-041]], num segundo ponto.

### O cache invalida corretamente

`IndexCache` é chaveado por `(index_id, generation)` (`lexical_core/src/cache.rs:1,15`), e `bump_generation`
sobe a cada `bm25_build`. Um rebuild com o analisador novo invalida o cache por construção.

### A linha de base a superar

`wiki/benchmarks/b040-theodb-fts-msmarco.md`, MS MARCO 100K, droplet `g-16vcpu-64gb`:
**NDCG@10 0,6962 · recall@10 0,8025 · MRR 0,667 · 1.616 QPS de pico · build do índice ~2,5 s.**

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `theodb_rs/src/lexical/engine.rs` | `build_schema` + registro do analisador em dois pontos de abertura |
| Índices já construídos | **nenhum efeito** — o schema deles diz `"default"`, que segue registrado |
| Índices novos | passam a stemizar; recall e NDCG mudam (para cima, é a hipótese) |
| Superfície SQL | **inalterada** — `bm25_build`/`bm25_search` mantêm assinatura |
| Custo de build | o stemmer roda por token na indexação; impacto a medir (linha de base ~2,5 s para 100K) |
| Artefato público | `b040-theodb-fts-msmarco.md` é **atualizado**, não duplicado (ADR-0061) |
| Idioma | decisão nova: fixo, parâmetro ou GUC — hoje não existe conceito de idioma no pilar |

## Corner 4 — Verification

1. `jumping` casa `jumps` — teste que hoje falha.
2. Um índice construído com o schema antigo (`"default"`) continua respondendo como antes — provado por
   teste, não por raciocínio sobre serialização.
3. NDCG@10, recall@10 e MRR **medidos no mesmo caso e comando** do `b040`, antes e depois, publicados mesmo
   se não melhorarem.
4. O erro de parse deixa de ser engolido — `UnknownTokenizer` falha alto.
5. O custo de build é medido: quanto o stemmer acrescenta aos ~2,5 s de 100K documentos.

## Reclassificação

`suggested_mode: evolve` mantido. Não é defeito — é capacidade ausente, com decisão do owner já tomada.
O que a descoberta mudou é o **tamanho** (configuração, não implementação) e o **risco** (a migração some
por construção, em vez de precisar de tratamento).
