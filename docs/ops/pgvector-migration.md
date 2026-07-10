# Migração pgvector → tipo `vector` own-code do TheoDB (M70)

Como migrar um banco que HOJE usa o `vector` do pgvector para o tipo `vector` **own-code** do TheoDB
(M69/M70). O layout on-disk é byte-idêntico (M69, `docs/adr/0028`), mas o tipo próprio do M70 ocupa
`public.vector` — o **mesmo nome** do pgvector — então **os dois não coexistem** (colisão de nome de tipo).
A migração de uma instalação com pgvector é, portanto, uma **operação com janela de manutenção**.

> **Instalações novas (greenfield) — o caminho primário:** nada a fazer. `CREATE EXTENSION theodb CASCADE`
> já instala o tipo `vector` own-code (via `theodb_rs`), sem pgvector. Validado (M70): extensões instaladas
> `theodb` + `theodb_rs`, zero `vector`/`vectorscale`. Este playbook é SÓ para **upgrades** de bancos com
> colunas `vector` do pgvector.

## Por que não é um byte-cast direto (honestidade — Regra 3)

O cast binário `WITHOUT FUNCTION` que o M69 provou (`theodb.vector ↔ pgvector.vector`, byte-idêntico) exige
os **dois tipos coexistindo em schemas distintos**. No M70 o tipo próprio é `public.vector` — mesmo nome do
pgvector — então não é possível instalá-los juntos. A migração usa um **intermediário neutro (`real[]`)** que
qualquer um dos tipos converte. Isso **preserva os dados** (os floats sobrevivem no `real[]`), mas reescreve o
heap das colunas (não é O(1)) e exige janela.

## Procedimento (janela de manutenção; testado no design)

```sql
-- 0. BACKUP da base (DDL de produção — sempre).

-- 1. Converter cada coluna `vector` (pgvector) para o intermediário neutro `real[]`.
--    O cast vector→real[] é do pgvector; preserva os floats. (Reescreve o heap da coluna.)
ALTER TABLE minha_tabela ALTER COLUMN emb TYPE real[] USING emb::real[];

-- 2. Remover o pgvector (e o pgvectorscale). Agora sem colunas `vector` dependentes.
--    Isto também dropa os índices ANN do pgvector (hnsw/ivfflat/diskann) — serão recriados no passo 5.
DROP EXTENSION IF EXISTS vectorscale CASCADE;
DROP EXTENSION IF EXISTS vector CASCADE;

-- 3. Instalar o TheoDB (provê o tipo `public.vector` own-code — agora sem colisão).
CREATE EXTENSION theodb CASCADE;   -- puxa theodb_rs (o tipo + os AMs + os schemas)

-- 4. Converter as colunas de volta para o tipo `vector` own-code.
--    O cast real[]→vector é own-code (theodb_vector_from_real_array). Rejeita NaN/Inf; valida dim.
ALTER TABLE minha_tabela ALTER COLUMN emb TYPE vector USING emb::vector;

-- 5. Recriar os índices ANN sobre os AMs do TheoDB.
CREATE INDEX meu_indice_ann ON minha_tabela USING theodb_hnsw (emb);   -- ou theodb_ivfflat
```

## Caveats honestos

- **Janela de manutenção obrigatória:** os `ALTER COLUMN TYPE` (passos 1 e 4) reescrevem o heap da coluna
  (round-trip via `real[]`) e pegam `ACCESS EXCLUSIVE lock` na tabela. Não é o byte-cast O(1) — os dados são
  preservados, mas há reescrita. Agende numa janela de baixa carga.
- **REINDEX obrigatório** (passo 5): as opfamilies do pgvector (hnsw/ivfflat/diskann) ≠ as do TheoDB
  (theodb_hnsw/theodb_ivfflat) — são AMs distintos. O índice ANN é recriado (custo de um `CREATE INDEX`).
- **Ordem importa:** converter as colunas (passo 1) ANTES de dropar o pgvector (passo 2); reinstalar (passo 3)
  ANTES de converter de volta (passo 4). Fora de ordem, o `DROP EXTENSION vector` falha por dependência ou o
  `::vector` do passo 4 não resolve.
- **Downtime:** por causa da reescrita + REINDEX, esta migração NÃO é online. Para tabelas enormes, considere
  uma abordagem por-partição ou uma cópia lado-a-lado.

## Follow-up conhecido (dívida honesta)

Uma migração **byte-level sem reescrita** (aproveitando o layout idêntico) exigiria instalar o tipo próprio
num schema temporário (`theodb.vector`) durante a transição + `ALTER TYPE … SET SCHEMA public` após dropar o
pgvector. Isso NÃO está implementado no M70 (o tipo é fixo em `public.vector`). Rastreado no backlog como
otimização — o procedimento `real[]`-intermediário acima é o caminho correto e seguro hoje.

## Referências

- `docs/adr/0028-m69-own-vector-type.md` (o tipo own-code + a prova byte-idêntica)
- `docs/adr/0029-m70-drop-pgvector.md` (a remoção total + o flip)
