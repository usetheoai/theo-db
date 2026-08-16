---
item: B-031
mode: evolve
date: 2026-08-12
verdict: pending
---

# B-031 — A cadeia de upgrade custa 10.283 linhas e não é verificada por nada

## Corner 1 — Evidence

Medido em 2026-08-12 sobre `sql/` e `theodb_rs/sql/`.

### O custo, em linhas

| Cadeia | Arquivos | Linhas | Forma |
|---|---|---|---|
| `theodb_rs/sql/theodb_rs--*.sql` | 4 | 9.785 | re-emissão convergente do SQL de instalação inteiro |
| `sql/theodb--*--*.sql` | 6 | 366 | deltas escritos à mão |
| `sql/vector--*.sql` | 3 | 132 | install 0.5.1 + delta + install 0.6.0 |
| **Total** | **13** | **10.283** | |

### A duplicação, medida par a par

`sort` + `diff` sobre os quatro elos do `theodb_rs`:

| Par | Linhas divergentes | Tamanhos |
|---|---|---|
| `1.0.0--1.1.0` → `1.1.0--1.2.0` | 55 | 2391 / 2444 |
| `1.1.0--1.2.0` → `1.2.0--1.3.0` | 274 | 2444 / 2478 |
| `1.2.0--1.3.0` → `1.4.0--1.5.0` | 148 | 2478 / 2472 |
| `1.0.0--1.1.0` → `1.4.0--1.5.0` | **339** | 2391 / 2472 |

O primeiro e o último elo — quatro versões de distância — são **86% idênticos**. `theodb_rs--1.3.0--1.4.0.sql` foge do padrão com 25 linhas: é delta real escrito à mão, o que mostra que o gerador nunca foi aplicado de forma consistente.

### A duplicação no umbrella, que é de outra natureza

`sql/theodb--1.5--1.6.sql` redefine `theodb.htap_refresh` e `theodb.olap`, que também são definidas em `sql/85-theodb-htap.sql`. O próprio arquivo declara, na linha 6:

> `-- e sql/85-theodb-htap.sql (a fonte greenfield — este delta re-aplica em intenção byte-idêntica).`

**"Em intenção"** é o ponto: nada verifica a identidade. Toda mudança nessas funções precisa ser escrita duas vezes, e a divergência não produz erro.

### O motivo da cadeia, e por que deixou de valer

`theodb_rs/sql/theodb_rs--1.3.0--1.4.0.sql`, linhas 12-16, declara textualmente:

> *"CONTEXTO (2026-08-08): o projeto está em PRÉ-RELEASE e não há instalação em campo. Este script existe por dois motivos que não dependem disso: (a) o `schema-drift-gate.yml` bloqueia mudança de superfície SQL sem bump de `default_version` ou script de migração […]; (b) a cadeia de upgrade é append-only."*

Ambos caíram:

- **(a) O gate não roda.** `schema-drift-gate.yml:87,88` invoca `scripts/sql-surface.sh`, removido em `8605677`.
- **O gerador não existe.** `scripts/gen-upgrade-script.py` saiu no mesmo commit — a cadeia não pode mais ser estendida pelo caminho documentado, que os arquivos gerados citam no cabeçalho ("GERADO por scripts/gen-upgrade-script.py […] NÃO editar à mão").

### A premissa "não há instalação em campo" — medida, não assumida

| Verificação | Resultado |
|---|---|
| `gh run list --workflow publish-image.yml --limit 10` | **10 de 10 falharam**, o mais recente em 2026-08-12 (run `31593313873`) |
| `GET ghcr.io/v2/usetheoai/theo-db/manifests/latest` | `401 UNAUTHORIZED`; o token anônimo devolve `UNAUTHORIZED` → pacote não é público |
| `publish-image.yml`, linhas 10-16 | documenta que o `uses:` apontava para org inexistente e que **por isso a imagem nunca foi publicada** |

Existem 170 tags e releases no GitHub (v0.158.0 é a última), mas são fonte, não binário instalável. **O primeiro comando do README — `docker pull ghcr.io/usetheoai/theo-db:latest` — nunca funcionou.**

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`. Nenhuma restrição de fluxo foi declarada para o ecossistema, e afirmar uma aqui seria asserção, não medição.

## Corner 3 — Blast radius

Contido ao próprio repositório.

| Alcance | Detalhe |
|---|---|
| `Dockerfile:109-113` | instala os scripts na extension dir (via glob, desde `d0771b3`) |
| `theodb.control` / `theodb_rs.control` / `vector.control` | `default_version` define o alvo do `CREATE EXTENSION` |
| `schema-drift-gate.yml` | já quebrado; consome `sql-surface.sh` removido |
| Repos irmãos | **nenhum** consome a extensão — `theo-rag` e `theo-memory` usam `pgvector` (registrado em `rules/dogfood-golden-rule.md`) |

Nenhum consumidor externo: sem imagem publicada, não há instalação a migrar.

## Corner 4 — Verification

1. `CREATE EXTENSION theodb_rs` numa base limpa entrega a superfície completa, e `schema_snapshot.sql` sobre ela produz o mesmo conjunto de objetos de antes da mudança.
2. Nenhum arquivo `*--*--*.sql` permanece nas duas árvores.
3. `Dockerfile` constrói e o initdb sobe sem erro.
4. O CHANGELOG declara que a cadeia saiu e sob qual premissa.

**Limite declarado:** `schema_snapshot.sql` registra **membresia** (`pg_depend`), não ACL — ele mesmo declara isso. Um upgrade que perdesse um `REVOKE ... FROM PUBLIC` passaria. Como este item **remove** a cadeia em vez de mantê-la, o ponto cego deixa de ser risco de upgrade; mas a verificação de ACL do greenfield passa a ser obrigação do B-030.

## Reclassificação

`suggested_mode: evolve` mantido. A medição confirmou custo do status quo (10.283 linhas duplicadas) sem alterar a natureza da hipótese.
