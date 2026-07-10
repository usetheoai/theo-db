# Migração pgvector → tipo `vector` own-code do TheoDB (M70)

Playbook para migrar uma instalação que HOJE usa o `vector` do pgvector para o tipo `vector`
**own-code** do TheoDB (M69/M70), **sem downtime e sem reescrita de dados** — o layout on-disk é
byte-idêntico (provado no M69: `docs/adr/0028`, gate `binary_compat_with_pgvector` sobre `md5`).

> **Instalações novas (greenfield):** nada a fazer — `CREATE EXTENSION theodb CASCADE` já instala o
> tipo `vector` own-code (via `theodb_rs`), sem pgvector. Este playbook é só para **upgrades** de
> bancos que já têm colunas `vector` do pgvector.

## Pré-condição

O layout do `vector` do TheoDB é `#[repr(C)] { varlena u32 · dim u16 · unused u16 · f32[] }` — **idêntico
byte-a-byte** ao `Vector` do pgvector. Por isso um cast binário (`WITHOUT FUNCTION`) reinterpreta os bytes
sem reescrever o heap.

## Passos (ordem importa)

```sql
-- 1. Instalar o theodb_rs (provê o tipo `vector` own-code). Se o pgvector ainda estiver instalado, o tipo
--    próprio do TheoDB coexiste como `theodb.vector` durante a transição (M69). Após remover o pgvector
--    (passo 5), o tipo próprio ocupa `public.vector` (drop-in). Em greenfield já nasce `public.vector`.
CREATE EXTENSION IF NOT EXISTS theodb_rs;   -- ou: CREATE EXTENSION theodb CASCADE;

-- 2. Declarar o cast binário GRÁTIS entre o `vector` do pgvector e o tipo próprio (requer superuser).
--    WITHOUT FUNCTION = reinterpretação de bytes (layout idêntico) — O(1), sem reescrita.
CREATE CAST (vector AS theodb.vector) WITHOUT FUNCTION AS IMPLICIT;

-- 3. Migrar cada coluna `vector` das tabelas de usuário. NÃO reescreve o heap (binary-coercible).
--    Faça isto ANTES de dropar o pgvector (passo 5) — senão o DROP falha por dependência.
ALTER TABLE minha_tabela ALTER COLUMN emb TYPE theodb.vector USING emb::theodb.vector;

-- 4. REINDEX dos índices ANN. NECESSÁRIO: as operator families do pgvector (hnsw/ivfflat/diskann) diferem
--    das do TheoDB (theodb_hnsw/theodb_ivfflat). Recrie o índice sobre o tipo próprio:
DROP INDEX IF EXISTS meu_indice_ann;
CREATE INDEX meu_indice_ann ON minha_tabela USING theodb_hnsw (emb);   -- ou theodb_ivfflat

-- 5. Remover o pgvector (e o pgvectorscale, se presente). Agora sem dependentes.
DROP EXTENSION IF EXISTS vectorscale CASCADE;
DROP EXTENSION IF EXISTS vector CASCADE;
```

## Caveats honestos

- **REINDEX é obrigatório** (passo 4): a migração da COLUNA é grátis (byte-cast), mas os índices ANN
  precisam ser recriados sobre os AMs do TheoDB (as opfamilies do pgvector não são compartilhadas). O
  rebuild do índice tem o custo normal de um `CREATE INDEX` (proporcional ao nº de vetores).
- **Janela de escrita:** o `ALTER COLUMN TYPE` pega um `ACCESS EXCLUSIVE lock` breve na tabela (o cast é
  O(1), mas o lock existe). Para tabelas quentes, agende numa janela de baixa carga.
- **Nome do tipo:** durante a coexistência (pgvector ainda instalado), o tipo próprio é `theodb.vector`.
  Após dropar o pgvector, um upgrade do TheoDB relocaliza o tipo para `public.vector` (drop-in — `::vector`
  do usuário resolve ao tipo próprio). Em instalações novas já nasce `public.vector`.
- **Backup:** como sempre em DDL de produção, tenha um backup antes. O cast binário é seguro (não altera
  bytes), mas o REINDEX e o `ALTER` são operações de esquema.

## Por que é seguro

O `md5` do `send` (wire binário) do tipo próprio é idêntico ao do pgvector em qualquer dimensão (provado no
M69, dims 1/3/5/128/300). O layout on-disk é o mesmo. O cast `WITHOUT FUNCTION` só troca o rótulo do tipo —
os bytes não mudam. Ver `docs/adr/0028-m69-own-vector-type.md` e `docs/adr/0029-m70-drop-pgvector.md`.
