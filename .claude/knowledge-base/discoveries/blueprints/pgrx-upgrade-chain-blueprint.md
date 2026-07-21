---
slug: pgrx-upgrade-chain
milestone_id: M137
date: 2026-07-21
---

# Blueprint — cadeia `ALTER EXTENSION UPDATE` para extensão pgrx

Fontes primárias: README do pgrx 0.19 e do cargo-pgrx, `extension.c` do PostgreSQL (clone local),
clones locais de paradedb / pgvectorscale / pgvector, e medição do nosso próprio histórico git.

## Coverage Corner 1 — Integration tests

**pg_durable tem o modelo mais rico** (`scripts/test-upgrade.sh`), com três cenários — e o segundo é o que
ninguém pensa:

- **A** — schema pós-`ALTER EXTENSION UPDATE` == schema de `CREATE EXTENSION` fresco.
- **B1** — o **`.so` NOVO contra o catálogo ANTIGO, sem rodar o update**. É o usuário que faz `apt upgrade` e
  esquece o `ALTER EXTENSION`. Para nós isso **não é opcional**: nossos index AMs leem páginas em disco, então
  divergência de assinatura ali é crash, não erro.
- **B2** — dado criado sob a versão antiga continua consultável após o upgrade.

**ParadeDB** rebuilda a versão antiga **a partir da tag git**, não de artefato publicado
(`.github/actions/test-pg_search-upgrade/action.yml`), instalando inclusive o `cargo-pgrx` da época. Casos de
teste são pastas com `setup.sql` (roda no antigo) e `queries.sql` (roda pós-upgrade). Caveat que eles próprios
documentam: rodar a suíte de integração pós-upgrade prova símbolos, **não** prova estado em disco.

## Coverage Corner 2 — Dependencies

`pg-schema-diff` (fork ParadeDB) para o gate de autoria. Nenhuma dependência nova em runtime — a cadeia é SQL.
Due-diligence D1 do `pg-schema-diff` fica como pré-requisito antes de vendorizar.

## Coverage Corner 3 — Tools

`cargo pgrx schema` emite o SQL de instalação **completo**, e a ordem **não é estável** (o header gerado diz
isso). Logo `diff(1)` entre dois schemas é inútil — precisa de differ semântico. O `check_migration_diff.py`
do ParadeDB contorna isso de forma deliberadamente rústica: tira comentários e meta-comandos, colapsa espaço,
quebra em `;`, filtra `^(CREATE|ALTER|DROP)` e compara **conjuntos** — checagem de subconjunto, não igualdade.

## Coverage Corner 4 — Techniques

### T1 — O pgrx NÃO gera scripts de upgrade (fato decisivo)

Confirmado em três fontes: o README do pgrx lista *"Automatic extension schema upgrade scripts"* sob `## TODO`;
o README do cargo-pgrx diz textualmente que **"pgrx has no ability to auto-generate these scripts"**; e o único
`ALTER EXTENSION` no código-fonte é o caminho `--no-schema` (adotar objetos existentes), não versionamento.

O que o pgrx **faz**: copia automaticamente `sql/*--*--*.sql` no `install`/`package`. Então o mecanismo existe;
o conteúdo é 100% nosso.

### T2 — Caminho ausente é ERRO ALTO; script incompleto é divergência SILENCIOSA

`extension.c:1415-1419` levanta `ERRCODE_INVALID_PARAMETER_VALUE` — *"extension has no update path"*. E
`find_update_path` roda **Dijkstra** sobre os scripts disponíveis, então ele encadeia sozinho. Já estar na versão
alvo é **NOTICE**, idempotente.

**O perigo não é o script faltando — é o script presente e incompleto**, que sobe sem erro e deixa o banco
estruturalmente diferente de uma instalação limpa.

### T3 — Dois modelos no campo, e o nosso caso escolhe o terceiro

| | ParadeDB `pg_search` | pgvectorscale |
|---|---|---|
| conteúdo | **deltas** escritos à mão, cadeia linear (119 arquivos) | **SQL de instalação completo**, gerado, fan-out N×N |
| menor arquivo | 1 linha (só o `\echo`, bump puro) | ~1.4k linhas, sempre |
| por quê | assinatura estável, `module_pathname` fixo | `.so` **versionado** força re-emitir tudo sempre |

pgvectorscale prova que os seis `*--0.9.0.sql` são **byte-idênticos** (mesmo md5) — geram um e espalham.

**Nós não pagamos o imposto do `.so` versionado**: `theodb_rs.control` tem `module_pathname = '$libdir/theodb_rs'`.
E a extensão umbrella `theodb` já usa deltas no repo (`DATA = $(wildcard sql/theodb--*--*.sql)`).

### T4 — A medição que inverte a recomendação (nossa, não do campo)

A pesquisa recomendou "deltas como o ParadeDB" assumindo que *"toda instalação está no mesmo catálogo 1.0.0 —
isso é um presente"*. **Medimos e é falso.** `pg_extern` ao longo das tags, com `default_version` congelado:

| tag | data | `pg_extern` |
|---|---|---|
| v0.10.0 | 2026-06-28 | 0 |
| v0.30.0 | 2026-07-02 | 25 |
| v0.60.0 | 2026-07-09 | 57 |
| v0.90.0 | 2026-07-16 | 71 |
| v0.120.0 | 2026-07-21 | 94 |

**`1.0.0` rotula pelo menos cinco catálogos diferentes.** A versão mentiu por 120 releases.

**Consequência de desenho:** não existe delta correto de `1.0.0→1.1.0`, porque não sabemos o que há no `1.0.0`
de cada instalação. O primeiro salto precisa ser **convergente**: um script total e idempotente que leva
*qualquer* catálogo rotulado 1.0.0 ao estado 1.1.0 — `CREATE OR REPLACE` no que é substituível, guarda de
existência no que não é, `DROP ... IF EXISTS` para o que já existiu e não existe mais. **Do 1.1.0 em diante,
deltas**, porque daí a versão volta a ser honesta.

Ou seja: pgvectorscale no primeiro salto (por necessidade medida), ParadeDB do segundo em diante (por parsimônia).

### T5 — Objetos sem `OR REPLACE` e a armadilha de ACL

`CREATE OR REPLACE` só existe para FUNCTION/VIEW/PROCEDURE/AGGREGATE. **TYPE, ACCESS METHOD, OPERATOR,
OPERATOR CLASS e CAST** dão `42710 duplicate_object` — pgvectorscale guarda cada um com `DO $$ IF NOT EXISTS
(SELECT FROM pg_am/pg_opclass/pg_operator ...) $$`. Isso nos atinge direto: possuímos o tipo `vector` e os AMs
`theodb_ivfflat`/`theodb_hnsw`/`theodb_symqg`/`theodb_columnar` com suas opclasses.

E: `CREATE OR REPLACE FUNCTION` **preserva** owner e ACL; `DROP`+`CREATE` **perde**. Temos `REVOKE ... FROM
PUBLIC` em superfície sensível — qualquer troca de assinatura precisa re-emitir o `REVOKE` no mesmo script.

### T6 — Provar "pós-upgrade == instalação limpa" sem ferramenta

Os dois peers convergiram na mesma técnica, e são ~8 linhas de SQL:

```sql
SELECT pg_describe_object(d.classid, d.objid, d.objsubid) AS object
FROM pg_depend d JOIN pg_extension e ON e.oid = d.refobjid
WHERE d.refclassid = 'pg_extension'::regclass AND d.deptype = 'e'
  AND e.extname = 'theodb_rs' ORDER BY 1;
```

`pg_describe_object` devolve identificador qualificado e **sem OID**, comparável entre bancos; `ORDER BY 1` mata
a instabilidade de ordem. Snapshot do banco atualizado vs snapshot de um `createdb` limpo, `diff -u`. ACL não
aparece em `pg_depend` — se quisermos paridade de ACL, snapshot separado de `proacl`.

## ADRs

**ADR-1 — Primeiro salto convergente, deltas depois.** Alternativas: (a) deltas puros desde o início —
rejeitada, é incorreta com origem ambígua (T4); (b) fan-out N×N permanente estilo pgvectorscale — rejeitada,
8,5k linhas duplicadas e crescimento quadrático, e não pagamos o imposto que os obriga a isso (T3).

**ADR-2 — Baseline honesto.** Instalações anteriores a este milestone convergem pelo script total; não há
caminho retroativo para catálogos que divergiram *antes* de 1.0.0 existir como rótulo confiável. Declarado na
doc de migração, não escondido.

## Referências

- `~/.cargo/registry/src/*/pgrx-0.19.0/README.md` (§ TODO), cargo-pgrx README
- `knowledge-base/references/postgres/src/backend/commands/extension.c:1415-1428, 3228-3232`
- `knowledge-base/references/paradedb/pg_search/sql/` (119 deltas), `.github/scripts/check_migration_diff.py`
- `knowledge-base/references/pgvectorscale/sql/` (fan-out byte-idêntico), `pgvector/sql/`
- `sql/theodb--1.0--1.1.sql` … `--1.3--1.4.sql` (precedente in-repo)
