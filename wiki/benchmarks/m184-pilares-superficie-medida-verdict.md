---
type: Measurement
title: m184 — superfície, performance e cobertura por pilar: três divergências da tabela de maturidade
description: O SymQG está no binário default e tem build 3,5× mais lento; o lakehouse tem zero testes próprios contra uma nota que exigia testado; e os opclasses documentados não existem com o nome do pgvector.
resource: benchmarks/artifacts/m184/pillar-surface-measured.json
tags: [benchmark, m184, pilares, maturidade, divergencia, catalogo, superficie-sql, parcial]
milestone: M184
generated: { by: claude-code/opus-5, at: 2026-08-08T05:00:00Z }
sources:
  - id: surface
    resource: benchmarks/artifacts/m184/pillar-surface-measured.json
    title: Catálogo do PostgreSQL consultado no binário default, CPU dedicada
---

**Entrega parcial do M184.** Mede o eixo mais barato e mais objetivo da tabela de maturidade — *o que o
usuário de fato recebe no binário default* — consultando o catálogo do PostgreSQL em vez de ler
documentação. É exatamente o método que a tabela original **não** usou.

# O método

Droplet `c-8` de CPU dedicada, imagem `ghcr.io/usetheodev/theo-db:0.139.0`,
`CREATE EXTENSION theodb CASCADE`, e então `pg_am` / `pg_proc` consultados. Nenhuma feature flag.

# O que o binário default expõe

| access method | tipo |
|---|---|
| `theodb_columnar` | table |
| `theodb_hnsw` | index |
| `theodb_ivfflat` | index |
| **`theodb_symqg`** | **index** |

| superfície de função | quantidade |
|---|---|
| grafo (`graph`/`bfs`/`ppr`) | **23** |
| parquet / lakehouse | **4** |
| **lexical / BM25** | **0** |

# A primeira divergência medida

**O SymQG está registrado como access method no binário default.** A tabela de 2026-08-07 o classificou
como **1 — "experimental, não recomendado como default"**, lendo o `feature_status` da wiki.

As duas coisas são compatíveis em texto e não em consequência: *não recomendado* não é o mesmo que
*ausente*, e a nota 1 foi atribuída como se fosse. Um usuário pode escrever `USING theodb_symqg` hoje,
sem feature flag, e receber o índice que o [e2](/benchmarks/e2-symqg-inpg-verdict.md) mediu como **2,6–3,9×
mais lento** que a alternativa a recall casado.

Isso **agrava** o M176 em vez de aliviá-lo: não é código morto atrás de uma flag, é superfície pública
medida como pior. A decisão de promover-ou-aposentar deixa de ser higiene de repositório e passa a ter
consequência para quem instala.

**Confirmações, ditas com a mesma clareza:** o pilar lexical tem **zero** funções expostas — a nota 2
("fora do binário default") estava certa, e agora por catálogo, não por leitura do `Cargo.toml`. Grafo e
lakehouse estão presentes e amplos (23 e 4 funções), coerentes com as notas 3.

# Eixo performance — medido em CPU dedicada

20 000 vetores 128d, 200 000 linhas colunares, droplet `c-8` efêmero:

| pilar | operação | tempo |
|---|---|---|
| **vetorial** | build `theodb_hnsw` (20k) | 4 579 ms |
| | busca ANN top-10 | **3,7 ms** |
| | build `theodb_symqg` (20k) | **16 056 ms — 3,5× mais lento que o HNSW** |
| **colunar** | `INSERT` 200k | 452,6 ms |
| | `GROUP BY` | 144,6 ms |
| | `min`/`max` | 104,8 ms |
| lexical nativo (`tsvector`) | busca em 20k | 30,1 ms |

# Eixo cobertura de teste — contagem no fonte

310 testes no total. Por pilar, os extremos importam mais que a soma:

| pilar | testes |
|---|---|
| `am/` (colunar + AM) | 87 |
| `vec/` + `ann/` (vetorial) | 60 |
| grafo (4 arquivos) | 35 |
| vectorizer | 26 |
| lexical | 6 |
| **`embed.rs`** | **1** |
| **`rerank.rs`** | **1** |
| **`parquet.rs`** | **0** |

# Eixo crash-safety — EXECUTADO, não inventariado

`kill -9` no postmaster (PID 1 do container), restart, e comparação de checksum antes/depois. Recovery
confirmado no log do servidor.

| pilar | antes | depois | veredito |
|---|---|---|---|
| **vetorial** | 5 000 linhas | 5 000 linhas | busca ANN devolve 10 resultados pós-recovery — **índice utilizável** |
| **colunar** | `md5 d89aa957…` | `md5 d89aa957…` | **idêntico** |
| heap (controle) | `md5 d89aa957…` | `md5 d89aa957…` | idêntico |
| **grafo** | 1 000 arestas | 1 000 arestas | preservado |

**Verde nos quatro.** E um resultado colateral que vale por si: o `theodb_columnar` produz **md5
byte-idêntico ao heap** sobre os mesmos 50 000 registros — a propriedade que o
[m128](/benchmarks/m128-clickbench-columnar.md) mediu em 43 queries do ClickBench, aqui reconfirmada por
outro caminho, e **também depois do crash**.

Isto substitui o inventário anterior: os 17 scripts de `isolation/` continuam sem rodar (exigem
instalação pgrx do fonte), mas a propriedade que eles protegem foi **verificada por execução direta**.

# As divergências, em ordem de gravidade

**1. O SymQG está no default** (acima) — e o eixo de performance agrava: além de ser 2,6–3,9× mais lento
na busca ([e2](/benchmarks/e2-symqg-inpg-verdict.md)), o **build é 3,5× mais lento** que o HNSW no mesmo
dataset. Nota atribuída 1; a realidade é superfície pública com custo medido em dois eixos.

**2. O lakehouse tem zero testes próprios.** `parquet.rs` expõe 4 funções e tem **0** `#[test]`/`#[pg_test]`
— há apenas 2 testes citando parquet no `lib.rs`. A nota **3** exigia "testado". Existe
`isolation/crash_parquet.sh`, então crash-safety está coberta; **cobertura de teste unitário, não.** A
nota estava alta.

**3. Os opclasses documentados não são os reais.** `USING theodb_hnsw (v vector_l2_ops)` **falha** —
o nome é `theodb_hnsw_l2_ops`. Quem seguir a nomenclatura do pgvector recebe
`operator class does not exist`. Não é divergência de nota, é de documentação, e só aparece executando.

**Confirmações:** vetorial com 60 testes e busca em 3,7 ms sustenta a nota 4. Colunar com 87 testes no
`am/` sustenta a 3. Lexical com 6 testes e zero funções expostas sustenta a 2.

# O que este artefato NÃO mede

Cobre **quatro** dos cinco eixos: presença, performance, cobertura de teste e crash-safety executada.

**Não** mede **qualidade de recuperação por pilar** — e é uma lacuna com razão estrutural, não descuido:
qualidade de recuperação só é definível onde há recuperação. O vetorial tem (M45/M177), o lexical teria
mas **expõe zero funções**, e colunar, lakehouse e grafo não recuperam nada — para eles, o análogo é
**correção**, medida acima como md5 idêntico ao heap. O único pilar com o eixo genuinamente aberto é o
**híbrido**, cuja qualidade o [m123](/benchmarks/m123-hybrid-significance.md) já mediu como
não-significativa sobre o vetorial puro.

O dataset é **sintético e pequeno** (5k–200k linhas). Os tempos servem para **comparar pilares entre si
na mesma máquina**, não como números publicáveis de capacidade — para isso existem os artefatos de escala
do M45 e do ClickBench.

O dataset é **sintético e pequeno** (20k vetores, 200k linhas). Os tempos servem para **comparar pilares
entre si na mesma máquina**, não como números publicáveis de capacidade — para isso existem os artefatos
de escala do M45 e do ClickBench.

Um limite honesto de método: contar `pg_extern` no fonte e contar `pg_proc` no catálogo **discordam** —
`graph.rs` tem 9 `pg_extern` e o catálogo mostra 23 funções com nome de grafo, porque o `api.rs` é um
facade único ([ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)) e há SQL declarativo em
`extension_sql!`. **O catálogo é a fonte de verdade**; a contagem no fonte subestima.

# Relacionados

- A tabela que este milestone audita: § Roadmap v7 do `ROADMAP.md`
- O veredito que mediu o SymQG como mais lento: [e2](/benchmarks/e2-symqg-inpg-verdict.md)
- O milestone que decide promover ou aposentar: M176
- A restrição de facade que explica a divergência de contagem: [ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)
