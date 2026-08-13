---
slug: b036-build-reloptions
items: [B-036]
date: 2026-08-13
base: 96fb342
head: cecd388
verdict: READY_TO_MERGE
---

# Review — a metade que faltava, e os dois call sites que a descoberta não viu

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte Rust | **478 passed, 0 failed** (baseline 469 + 9 novos) |
| 2 | `cargo clippy` sob `-D warnings` com o baseline do `.clippy_args` | **limpo** |
| 3 | `cargo fmt --check` nos arquivos deste ciclo | **0 diffs** |
| 4 | Segredos commitados | **0** (hook confirmou) |
| 5 | Commit direto em `main` | não — `workspace` |
| 6 | Trailer de coautoria | **0** |
| 7 | `CHANGELOG.md` atualizado | sim — 3 entradas em `Added`, 3 em `Changed` |

`/code-quality`: `FAIL_SOFT`, **0 achados HARD**. Os 4 achados são os mesmos de ambiente dos três ciclos
anteriores (`cargo-udeps` sem permissão de escrita no `target/`, `pg_sys` não verificável em crates.io,
`Layerfile.toml` não declarado) e **nenhum toca um arquivo deste ciclo**.

## Cross-validation — 5 de 5, medidas no produto

Construí `theodb:b036` e rodei contra ele. É o que separa "a suíte passa" de "o usuário consegue".

| # | Afirmação | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | A sintaxe do pgvector passa a funcionar | `CREATE INDEX … USING hnsw (e vector_l2_ops) WITH (m=32, ef_construction=200)` no contêiner | `CREATE INDEX` — antes era `ERROR: unrecognized parameter "m"` |
| G2 | Aceito **e honrado** | `pg_relation_size`: **248 kB** (m=32) contra **152 kB** (m=16), mesmo corpus | ok — mais slots de vizinho por nó, que é o efeito de `m` no layout |
| G3 | Fora de faixa recusado nomeando a opção | `WITH (m=999999)` | `ERROR: value 999999 out of bounds for option "m"` / `DETAIL: Valid values are between "2" and "39"` |
| G4 | Índices existentes não mudam | `create_index_without_options_keeps_the_shipped_default` → `meta` = (16, 32) | ok — `rd_options` nulo devolve o default |
| G5 | A env var sumiu | `grep -c 'env::var("THEODB_HNSW_EF_CONSTRUCTION'` em `theodb_rs/src/` | **0** |

E o índice construído com `m=32, ef_construction=200` devolve **recall@10 = 1,0** contra o oráculo exato
(seqscan) sobre 500 vetores distintos.

## Achados

### R-1 — ALTO · A descoberta mapeou dois call sites; existem quatro

A oportunidade enumerou `build.rs:416` (build inicial) e `build.rs:706` (fold do VACUUM). Uma varredura de
consumidores antes de editar achou mais dois, e **os dois que faltavam eram os que quebrariam em silêncio**:

| Call site | O que teria acontecido |
|---|---|
| `hnsw_page/store.rs:383` — **INSERT** | um índice criado com `ef_construction=200` voltaria a 64 a cada linha inserida depois do build. Como `efc` **não é persistido em lugar nenhum**, nada no disco denunciaria |
| `cost.rs:123` — **estimativa de custo** | o planner seguiria estimando um grafo `m=16` para um índice `m=32`. O comentário do arquivo afirmava textualmente que `m` era constante — a afirmação virou falsa no instante em que a reloption existiu |
| `build.rs:843` — `ambuildempty_hnsw` | o índice **vazio** grava um meta, e é ele que o primeiro INSERT lê |

O padrão vale registrar porque não é sobre este item: **uma oportunidade que enumera call sites está fazendo
uma afirmação de completude que ninguém verificou**. Aqui o `grep` custou dois minutos e achou 50% a mais.

### R-2 — ALTO · A decisão que dispensou o bump de versão do meta

A oportunidade propunha persistir `ef_construction` no meta (com bump de versão), porque sem isso "um VACUUM
reconstruiria o índice com um valor diferente do de criação — silenciosamente".

O diagnóstico estava certo; a solução era mais cara que o necessário. O `sbq_bits` precisa do meta porque o
codebook **já está gravado nas páginas** — o fold tem de respeitar o que existe. O fold do HNSW
(`build.rs:697-709`) **reconstrói o grafo inteiro** a partir de `live`: qualquer `m` produz um grafo novo e
autoconsistente, e não há nada no disco para respeitar. Ler a reloption resolve o mesmo problema sem uma quinta
versão de meta.

E o argumento não ficou como argumento: `the_vacuum_fold_rebuilds_with_the_requested_m_not_the_default` cria
com `m=8`, chama o fold pela mesma FFI que o `ambulkdelete` percorre, e relê o meta do disco. Com a constante,
viraria (16, 32).

### R-3 — MÉDIO · O teto de `m` é derivado, não copiado

O pgvector aceita `m` até 100. Copiar esse número teria sido o caminho fácil e estaria **errado aqui**: nosso
`HNSW_MAX_LEVEL` é 32, então no pior caso um nó ocupa `32·m + m0 = 34m` slots de 6 bytes, e a tupla de
vizinhos tem de caber em `USABLE = 8168`. Isso dá **39** — m=40 estoura em 8.172.

`m_above_the_page_layout_ceiling_is_rejected` fixa o limite pelo motivo, não pelo número: se o layout de página
mudar e ninguém recalcular, o teste é o que denuncia.

### R-4 — MÉDIO · Um critério de aceite meu reprovava a própria explicação (de novo)

T1.5 exigia `grep -c "THEODB_HNSW_EF_CONSTRUCTION" theodb_rs/src/` igual a 0. O comentário que **explica** a
remoção da variável cita o nome dela. Satisfazer a letra significaria apagar a explicação.

**É a segunda vez neste ciclo de itens** — o T1.1 do [[B-045]] tinha exatamente a mesma forma. A correção é a
mesma: o critério quis dizer "nenhuma **dependência**", e dependência é a leitura (`env::var`), não a menção.
Registrado por acréscimo no plano.

Que o mesmo erro tenha reaparecido oito dias depois diz que a lição não pegou: **um critério de aceite escrito
como `grep` de string casa com a documentação da mudança tão bem quanto com a mudança**. Vale como classe.

### R-5 — MÉDIO · A monotonicidade que o plano pediu, a nossa própria medição refuta

T1.2 exigia que o recall com `ef_construction=400` fosse **maior** que com `16`. Codificar isso teria afirmado
em teste uma propriedade que o projeto já mediu ser falsa: o M57 registrou que subir `efc` de 64 para 200
**piorou** o recall a 100k–500k (`build.rs:15-21`), e é um honest-negative do próprio acervo.

O teste ficou com `efc=4` contra `efc=400` num corpus de 300 nós, onde a busca gulosa com `efc=4` é pobre
demais para empatar — e `high > low` foi **medido**, não previsto. A substituição está no plano com a razão.

Os outros dois elos que o recall sozinho não fecharia, porque `efc` não é persistido:
`ef_construction_reloption_reaches_the_accessor` (SQL → acessor, determinístico) e
`the_builder_actually_consumes_ef_construction` (grafos diferentes para `efc` diferente, **com o controle de
determinismo ao lado** — sem ele a diferença poderia ser não-determinismo em vez de efeito do parâmetro).

### R-6 — BAIXO · `rustfmt --edition 2021` estragou 91 linhas antes de eu perceber

O crate declara `edition = "2024"` em `Cargo.toml` **e** em `rustfmt.toml`. Invocar `rustfmt` à mão com
`--edition 2021` reformatou blocos que não são meus (reordenou imports, expandiu `if/else` de uma linha,
empurrou comentários para a coluna 40). Desfeito com `cargo fmt` (que lê a config do crate) + `git restore` dos
quatro arquivos que eu não havia tocado.

Fica o registro: **`cargo fmt`, nunca `rustfmt` solto** — a diferença é silenciosa e o diff sai plausível.

### R-7 — INFORMATIVO · O que este item NÃO fez, de propósito

- **`theodb_ivfflat` aceita as duas opções novas em silêncio.** Os dois AMs compartilham um `RELOPT_KIND` e um
  `amoptions`, então o `theodb_ivfflat` já aceitava as seis opções exclusivas do HNSW antes deste item — e
  agora aceita oito. É pré-existente, é da família [[B-048]], e a Q3 do plano o declarou fora de escopo. Mas o
  item **piorou o número**, e isso merece estar escrito em vez de subentendido.
- **`ALTER INDEX … SET (m=…)` sem `REINDEX`** deixa o índice com o `m` de criação até o próximo fold. Declarado
  na D2 do plano, não testado.
- **`THEODB_HNSW_PARALLEL_THRESHOLD` permanece.** É knob de bissecção de contenção, não de qualidade, e não tem
  reloption equivalente. Removê-la seria escopo que ninguém pediu.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **O caminho de INSERT com `efc` não-default não foi exercitado.** `insert_inplace` só roda quando há
  tombstone reutilizável; a mudança está correta por leitura e o caminho tem cobertura existente, mas **não**
  com um `ef_construction` diferente do default. É a afirmação mais fraca deste ciclo.
- **Nenhuma medição de qualidade em escala.** Que `m=32` mude o grafo está provado; se `m=32` é *melhor* que
  `m=16` a 100k–1M é exatamente a pergunta do [[B-046]], que este item **destrava** e não responde.
- **Sem significância pareada.** Nada aqui é alegação comparativa.
- **Um só ambiente.** Contêiner local, PG 18.4, x86-64.
- **O CI segue vermelho** ([[B-029]]).

## Veredito

**`READY_TO_MERGE`.**

5 de 5 afirmações verificadas contra a imagem construída; 478 testes verdes; clippy limpo; 0 achados HARD.

**Ressalvas:** review do próprio implementador; o caminho de INSERT com `efc` não-default não foi exercitado;
e o item entrega a **capacidade** de variar `m`/`ef_construction`, não a evidência de que variar ajuda — essa é
a medição do B-046.
