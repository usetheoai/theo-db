---
type: Feature
title: Índice HNSW (theodb_hnsw)
description: Access method HNSW próprio, page-native com travessia sob demanda; o recall se ajusta em tempo de query por ef_search e, desde o B-036, a qualidade do grafo por m e ef_construction no CREATE INDEX.
resource: git:f7c7b93:docs/features/02-indice-hnsw.md
tags: [feature, indice, hnsw, ann, access-method]
feature_status: entregue
milestone: M21+M35+B-036
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat02
    resource: git:f7c7b93:docs/features/02-indice-hnsw.md
    title: Criar índices HNSW
---

**Status: entregue.** O TheoDB tem um access method [HNSW](/technologies/hnsw.md) **próprio**. Desde a
reestruturação page-native, a persistência é em páginas com travessia **sob demanda**, o que trocou um
custo O(N) por O(ef·M) — medido em [m35](/benchmarks/m35-hnsw-structured-scan.md), com paridade de
recall contra o HNSW do [pgvector](/technologies/pgvector.md) como baseline.

# Criar o índice

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;

CREATE INDEX products_hnsw
ON products
USING theodb_hnsw (description_embedding theodb_hnsw_cosine_ops);
```

| Opclass | Métrica | Operador correspondente |
|---|---|---|
| `theodb_hnsw_l2_ops` | L2 (euclidiana) — **default** | `<->` |
| `theodb_hnsw_cosine_ops` | cosseno | `<=>` |
| `theodb_hnsw_ip_ops` | produto interno | `<#>` |

A opclass **precisa casar** com o operador usado na consulta; caso contrário o índice não é usado.

Aplicações que escrevem a sintaxe do pgvector (`USING hnsw (col vector_cosine_ops)`) funcionam pelo
alias descrito no [ADR 0058](/decisions/0058-pgvector-compat-shim.md) — que aponta para **este mesmo
handler**, sem segunda implementação.

# Parâmetros de build

**`m` e `ef_construction` são opções de `WITH` desde o B-036.** Até então eram constantes de compilação
(16 e 64), e a segunda só mudava por variável de ambiente do servidor — o que fazia `CREATE INDEX … WITH
(m = 32)` falhar com `unrecognized parameter "m"`, a confusão mais provável para quem vinha do
[pgvector](/technologies/pgvector.md).

```sql
CREATE INDEX products_hnsw ON products
USING theodb_hnsw (description_embedding theodb_hnsw_cosine_ops)
WITH (m = 32, ef_construction = 200);
```

| Opção | Default | Faixa | O que controla |
|---|---|---|---|
| `m` | 16 | 2 – 39 | grau máximo de vizinhos acima do nível 0 (`m0 = 2m` no nível de solo) |
| `ef_construction` | 64 | 4 – 1000 | tamanho da lista de candidatos durante o build |

Os defaults **coincidem com os do pgvector**, o que mantém comparável toda medição já publicada. Valor
fora da faixa é **recusado nomeando a opção**, não truncado em silêncio.

**O teto de `m` é 39, e não o 100 do pgvector**, porque é derivado do nosso page layout: no pior caso um
nó ocupa `HNSW_MAX_LEVEL·m + m0 = 34m` slots de 6 bytes, e a tupla de vizinhos tem de caber nos 8.168
bytes úteis de uma página. Copiar o número do pgvector — cujo teto de nível é outro — daria um índice que
não cabe.

O valor pedido é honrado nos **quatro** caminhos que constroem grafo: o build inicial, o índice vazio de
tabela `UNLOGGED`, o INSERT posterior e o fold do VACUUM. Os dois últimos importam mais do que parece:
`ef_construction` **não é persistido em lugar nenhum**, então um índice que voltasse ao default a cada
INSERT — ou que o VACUUM reconstruísse com outro `m` — não teria nada no disco que denunciasse.

As demais opções de `WITH` são de **quantização e storage**, compartilhadas com os demais access methods:
`sbq_bits`, `pq_subspaces`, `pq_bits` (que só aceita `4`), `aq_threshold`, `separate_storage` e `refine`.
Sem nenhuma delas, o índice guarda os vetores em precisão plena. Ver
[quantização vetorial](/features/19-quantizacao-vetorial.md).

**Ressalva honesta:** o item entrega a *capacidade* de variar a qualidade do grafo, não a evidência de que
variar ajuda. Se `m=32` bate `m=16` a 100k–1M é pergunta em aberto, e é o que o benchmark do B-046 existe
para medir.

# O knob de recall é em tempo de query

```sql
SET theodb_hnsw.ef_search = 100;   -- maior = mais recall, mais latência
```

Para escolher o valor sem tentativa e erro, existe o recomendador determinístico decidido no
[ADR 0026](/decisions/0026-m67-autotune-recommender.md):

```sql
SELECT theodb.recommend_ef('products'::regclass, 'description_embedding', ARRAY[...], 0.95, 10);
```

Ele acha o **menor** `ef` que atinge o recall-alvo, por bisecção — o que é correto porque `recall(ef)`
é monotônico não-decrescente.

# Acompanhar a construção

```sql
SELECT phase FROM pg_stat_progress_create_index;
```

A fase `building graph` indica que o algoritmo está montando o grafo; ela desaparece ao terminar.

# Qualidade do grafo — o que a medição mostrou

Duas decisões medidas afetam diretamente o recall deste índice:

- **`extendCandidates` está ligado por padrão**
  ([ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md)), porque sem ele o recall
  degradava com a escala. O custo é um build 2 a 3× mais lento, com opt-out por variável de ambiente.
- O critério de recall do projeto é **paridade com o pgvector**, e não um valor absoluto
  ([ADR 0030](/decisions/0030-m60-recall-parity-not-absolute-099.md)) — porque o próprio pgvector não
  alcança 0,99 no corpus medido.
- **`ef_construction` maior não é sempre melhor.** O M57 mediu que subir de 64 para 200 **piorou** o
  recall a 100k–500k, e `m` de 16 para 32 também piorou (0,952). É por isso que a faixa é larga e o
  default é conservador: o knob agora existe para ser *varrido por medição*, não para ser subido no
  escuro.

# Manutenção

O VACUUM e a compaction deste índice seguem o desenho do
[ADR 0017](/decisions/0017-m55-index-maintenance-at-scale.md), com a garantia de crash-safety do
[ADR 0014](/decisions/0014-m48-crash-safe-fold-reclaim-mechanism.md). Em índices muito grandes, a
recuperação de espaço pode exigir `REINDEX` explícito, conforme registrado no
[ADR 0047](/decisions/0047-m104-scaling-tradeoffs-deliberate.md).

# Alternativas

O [índice IVFFlat](/features/03-indice-ivfflat.md) é a alternativa por listas invertidas, e o
[índice SymQG](/features/17-indice-symqg.md) é a linha experimental. Para diagnosticar recall baixo ou
latência alta em produção, ver o [runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md).
