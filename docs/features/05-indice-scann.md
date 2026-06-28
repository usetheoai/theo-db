# Criar um índice ScaNN

> **⚖️ Decisão (M14, 2026-06-28) — NO-FORK:** TheoDB **não** constrói o access method `theodb_scann` literal.
> O índice ScaNN-quality **entregue** é o **StreamingDiskANN** (`pgvectorscale`, M2) — substituto permissivo
> que atinge a barra ScaNN-quality (recall@10 medido **0.986** ≥ 0.90; `docs/benchmarks/m14-scann-fork-decision.md`).
> A decisão é gateada por evidência (measurement-first / anti-sunk-cost) — ver
> [`docs/adr/0004-scann-fork-decision.md`](../adr/0004-scann-fork-decision.md), que define o gatilho que
> reabriria o fork. A superfície `theodb_scann` literal abaixo permanece como API-alvo (gated, não entregue);
> use `USING diskann` (pgvectorscale) hoje.

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M2 — Vetorial / IA** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a criação de índices `ScaNN` no TheoDB via extensão `theodb_scann`, incluindo os modos automático e manual, os quantizadores suportados, parâmetros de árvore, manutenção do índice e exemplos de consulta vetorial.

---

# 1. Instalar a extensão ScaNN

```sql
CREATE EXTENSION IF NOT EXISTS theodb_scann CASCADE;
```

Instala a extensão `theodb_scann`. Caso `vector` ainda não esteja instalado, ele é instalado automaticamente.

---

# 2. Habilitar índice de quatro níveis (sessão)

```sql
SET scann.max_allowed_num_levels = 3;
```

Permite criar índices ScaNN de quatro níveis (Preview).

---

# 3. Criar índice ScaNN automático

```sql
CREATE INDEX my_scann_index
ON products
USING scann(description_embedding cosine);
```

Cria um índice totalmente gerenciado pelo TheoDB.

---

# 4. Criar índice automático com parâmetros

```sql
CREATE INDEX my_scann_index
ON products
USING scann(description_embedding cosine)
WITH (
    MODE='AUTO',
    OPTIMIZATION='SEARCH_OPTIMIZED',
    auto_maintenance='ON'
);
```

Permite configurar otimização e manutenção automática.

---

# 5. Modo AUTO

```sql
MODE='AUTO'
```

O TheoDB escolhe automaticamente toda a estrutura do índice.

---

# 6. Otimização SEARCH_OPTIMIZED

```sql
OPTIMIZATION='SEARCH_OPTIMIZED'
```

Prioriza recall e velocidade das consultas.

---

# 7. Otimização BALANCED

```sql
OPTIMIZATION='BALANCED'
```

Equilibra tempo de construção do índice e desempenho das consultas.

---

# 8. Auto manutenção ligada

```sql
auto_maintenance='ON'
```

O índice é reconstruído automaticamente quando necessário.

---

# 9. Auto manutenção desligada

```sql
auto_maintenance='OFF'
```

A manutenção passa a ser responsabilidade do DBA.

---

# 10. Criar índice manual (2 níveis)

```sql
CREATE INDEX my_scann_index
ON products
USING scann(description_embedding cosine)
WITH (
    mode='MANUAL',
    num_leaves=1000,
    quantizer='SQ8',
    auto_maintenance='ON'
);
```

Permite controle completo sobre a estrutura.

---

# 11. Criar índice manual (3 níveis)

```sql
CREATE INDEX my_scann_index
ON products
USING scann(description_embedding cosine)
WITH (
    mode='MANUAL',
    num_leaves=1000,
    quantizer='SQ8',
    max_num_levels=2
);
```

Cria árvore ScaNN com três níveis.

---

# 12. Criar índice manual (4 níveis)

```sql
CREATE INDEX my_scann_index
ON products
USING scann(description_embedding cosine)
WITH (
    mode='MANUAL',
    num_leaves=1000,
    quantizer='SQ8',
    max_num_levels=3
);
```

Cria árvore de quatro níveis (Preview).

---

# 13. Definir número de folhas

```sql
num_leaves=10000
```

Controla o número de partições utilizadas pelo índice.

---

# 14. Quantizer SQ8

```sql
quantizer='SQ8'
```

Compactação padrão com pequena perda de recall.

---

# 15. Quantizer AH

```sql
quantizer='AH'
```

Asymmetric Hashing, até 4× menor que SQ8.

---

# 16. Quantizer FLAT

```sql
quantizer='FLAT'
```

Maior precisão possível (>99%), porém consultas mais lentas.

---

# 17. Resetar parâmetros do índice

```sql
ALTER INDEX my_scann_index
RESET (num_leaves, quantizer);
```

Remove parâmetros manuais antes da conversão para AUTO.

---

# 18. Converter índice manual em automático

```sql
REINDEX INDEX CONCURRENTLY my_scann_index;
```

Reconstrói o índice utilizando configuração automática.

---

# 19. Criar índice para coluna `real[]`

```sql
CREATE INDEX my_scann_index
ON products
USING scann(
    CAST(description_embedding AS vector(768))
    cosine
);
```

Permite indexar embeddings armazenados como `real[]`.

---

# 20. Consultar progresso da indexação

```sql
SELECT *
FROM pg_stat_progress_create_index;
```

Mostra andamento da construção do índice.

---

# 21. Verificar fase da indexação

```sql
SELECT phase
FROM pg_stat_progress_create_index;
```

Mostra em qual etapa o índice está sendo criado.

---

# 22. Permitir criação diferida

```sql
CREATE INDEX my_scann_index
ON products
USING scann(description_embedding cosine);
```

Quando os recursos Preview estão habilitados, a criação pode ser adiada automaticamente até que haja dados suficientes.

---

# 23. Permitir operações bloqueadas

```sql
SET scann.allow_blocked_operations = true;
```

Força criação do índice mesmo em tabelas pequenas ou vazias.

---

# 24. Criar SUPERUSER

```sql
CREATE USER myuser
WITH SUPERUSER
PASSWORD 'password';
```

Necessário para forçar criação de índices em alguns cenários.

---

# 25. Consulta vetorial usando ScaNN

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'running shoes'
)::vector
LIMIT 10;
```

Consulta vetorial utilizando o índice ScaNN.

---

# 26. Consulta utilizando distância L2

```sql
SELECT *
FROM products
ORDER BY description_embedding
<-> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'running shoes'
)::vector
LIMIT 10;
```

Usa distância Euclidiana.

---

# 27. Consulta utilizando produto interno

```sql
SELECT *
FROM products
ORDER BY description_embedding
<#> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'running shoes'
)::vector
LIMIT 10;
```

Usa Inner Product.

---

# 28. Consulta utilizando distância cosseno

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'running shoes'
)::vector
LIMIT 10;
```

Usa Cosine Distance.

---

# 29. Buscar apenas o melhor resultado

```sql
LIMIT 1;
```

Retorna somente o vizinho mais próximo.

---

# 30. Buscar Top-N resultados

```sql
LIMIT 20;
```

Retorna os vinte vetores mais semelhantes.

---

# 31. Consulta com embedding literal

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> '[0.12,0.54,...]'::vector
LIMIT 5;
```

Pesquisa usando um vetor já calculado.

---

# 32. Consulta com texto

```sql
SELECT *
FROM documents
ORDER BY embedding
<=> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'refund policy'
)::vector
LIMIT 5;
```

Transforma texto em embedding durante a consulta.

---

# 33. Consulta combinada com filtro SQL

```sql
SELECT *
FROM products
WHERE category_id = 2
ORDER BY description_embedding
<=> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'hoodie'
)::vector
LIMIT 5;
```

Combina busca vetorial com filtros relacionais.

---

# 34. Consulta exibindo score

```sql
SELECT *,
       description_embedding
       <=> theodb_ml.embedding(
           model_id=>'theodb-embedding-005',
           content=>'hoodie'
       )::vector AS distance
FROM products
ORDER BY distance
LIMIT 5;
```

Retorna também a distância calculada.

---

# 35. Ajustar paralelismo da criação

Parâmetros PostgreSQL utilizados durante a construção do índice:

```sql
max_parallel_maintenance_workers
```

Número máximo de workers para manutenção.

---

# 36. Ajustar workers globais

```sql
max_parallel_workers
```

Define o número máximo de workers paralelos do banco.

---

# 37. Ajustar limite para paralelismo

```sql
min_parallel_table_scan_size
```

Define quando um scan paralelo pode ser utilizado.

---

# 38. Ajustar memória da construção

```sql
maintenance_work_mem
```

Quantidade de memória utilizada durante a criação do índice.

---

# 39. Ajustar memória compartilhada

```sql
shared_buffers
```

Memória compartilhada utilizada pelo PostgreSQL durante operações do índice.

---

# 40. Fluxo completo recomendado

```sql
CREATE EXTENSION theodb_scann CASCADE;

CREATE INDEX products_embedding_idx
ON products
USING scann(description_embedding cosine);

SELECT *
FROM products
ORDER BY description_embedding
<=> theodb_ml.embedding(
    model_id=>'theodb-embedding-005',
    content=>'wireless headphones'
)::vector
LIMIT 10;
```

Fluxo completo de uso do ScaNN no TheoDB: instalação da extensão, criação do índice e execução de uma consulta vetorial utilizando embeddings.
