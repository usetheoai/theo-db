# Criar um índice ScaNN

> **⚖️ Decisão (M14, 2026-06-28) — NO-FORK:** TheoDB **não** constrói o access method `theodb_scann` literal,
> gated por benchmark (measurement-first / anti-sunk-cost) — ver
> [`docs/adr/0004-scann-fork-decision.md`](../adr/0004-scann-fork-decision.md), que define o gatilho que
> reabriria o fork. A superfície `theodb_scann` / `USING scann (…)` desta página permanece como **API-alvo
> condicional (gated, não entregue)**.

> **Status (atual, pós-M70):** a capacidade **ScaNN-quality entregue** é o **`theodb_ivfflat` own-code** —
> `CREATE INDEX … USING theodb_ivfflat (col …_ops) WITH (lists = N, pq_subspaces = M)` — que combina listas
> invertidas (IVF) com quantização Asymmetric-Hashing e scan batched (LUT16/`pshufb`), atingindo **paridade de
> recall** classe-pgvector. O `pgvector`/`pgvectorscale` (`USING diskann`) foram **removidos no M70** — o tipo
> `vector` e os AMs ANN são 100% own-code (`theodb_rs`); qualquer instrução para "usar `USING diskann` hoje" é
> histórica e não se aplica mais. Veredito de performance **medido** (M73/M74, `docs/adr/0035`): superar o
> ScaNN/AlloyDB em **QPS vetorial** é estruturalmente **não-alcançável** por extensão PG permissiva (gap de
> paradigma ~25–44× @ 0.99) — a entrega é paridade de recall + memória, nunca "mais rápido que o ScaNN".

Esta página cobre a criação de índices `ScaNN` no TheoDB, incluindo os modos automático e manual, os quantizadores suportados, parâmetros de árvore, manutenção do índice e exemplos de consulta vetorial.

---

## ✅ Caminho SHIPPED — ScaNN-inspired via `theodb_ivfflat` (IVF-AQ+AH)

**Não** existe um access method `theodb_scann` / `USING scann` nem uma extensão
`theodb_scann`. A técnica ScaNN-inspired **entregue hoje** é IVF + Asymmetric Hashing
(AQ/AH) em código próprio, exposta pelo access method `theodb_ivfflat` com Product
Quantization habilitado via a reloption `pq_subspaces`:

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE INDEX products_scann_like_idx
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_l2_ops
)
WITH (
    lists = 1000,
    pq_subspaces = 16,
    pq_bits = 4,
    separate_storage = 1
);

SELECT *
FROM products
ORDER BY description_embedding
<=> theodb.embed('wireless headphones', 'text-embedding-3-small')
LIMIT 10;
```

> **Nota honesta (medida):** este caminho own-code alcança **paridade de recall**
> classe-pgvector. A **superioridade de QPS vetorial sobre o ScaNN/AlloyDB é
> MEDIDA como NÃO-ALCANÇÁVEL** por uma extensão PostgreSQL permissiva (gap ~25–44×
> @ 0.99 recall é de paradigma — AH-LUT anisotrópico + não pagar o imposto MVCC/WAL).
> Isso é um limite de paradigma documentado (`docs/adr/0035`, `docs/adr/0036`), **não**
> um gap a fechar. Nenhuma afirmação de desempenho sem artefato em `docs/benchmarks/`
> (regra TheoDB 5).

---

## 🎯 API-alvo / roadmap (não-shipped)

> **Não entregue.** Toda a superfície `USING scann (…)` / `CREATE EXTENSION theodb_scann`
> abaixo é **API-alvo condicional** — o access method `theodb_scann` literal **não é
> implementado por decisão explícita** (NO-FORK M14, gated por benchmark —
> [`docs/adr/0004-scann-fork-decision.md`](../adr/0004-scann-fork-decision.md)). Para a
> capacidade equivalente entregue hoje, use o caminho SHIPPED acima (`theodb_ivfflat`
> com `pq_subspaces`). Os exemplos a seguir descrevem a superfície-alvo, não a atual.

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
