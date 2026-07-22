# Criar um índice vetorial SymphonyQG (`theodb_symqg`)

> **✅ Entregue (E2) — AM alternativo / experimental:** o índice vetorial own-code `theodb_symqg` (grafo
> quantizado co-localizado, clean-room do arXiv:2411.12229 — a referência C++ NTUITIVE foi **estudo-apenas,
> nunca copiada**, D1). Registrado em `theodb_rs/src/am/mod.rs:87` (`CREATE ACCESS METHOD theodb_symqg TYPE
> INDEX HANDLER theodb_symqg_amhandler`); opclass **default** `theodb_symqg_l2_ops` **somente L2**
> (`theodb_rs/src/am/mod.rs:359-360`; um build não-L2 falha rápido em `theodb_rs/src/am/build.rs:456-459`);
> reloption `WITH (degree_bound = R)` (`theodb_rs/src/am/options.rs:199`); kernel FastScan 1-bit sob a GUC
> `theodb.symqg_fastscan` (default ON — `theodb_rs/src/am/guc.rs:121,336`). Provado por
> `benchmarks/e2_symqg_inpg.py` e `benchmarks/e2_symqg_fastscan_ablation.py`.
>
> **⚠️ Honestidade crítica (não é o default recomendado):** o veredito **medido** é que o
> `theodb_symqg` é **mais lento** que o `theodb_hnsw` in-PG. Em SIFT1M, com recall casado, o
> `theodb_hnsw` é **2,6–3,9× mais rápido** na faixa prática (recall@10 0,95–0,994) — o gate de superioridade
> **NÃO foi atingido** ([`docs/benchmarks/e2-symqg-inpg-verdict.md`](../benchmarks/e2-symqg-inpg-verdict.md)).
> O kernel FastScan 1-bit dá um ganho **modesto** de 1,07–1,22× no mesmo índice
> ([`e2-symqg-fastscan-verdict.md`](../benchmarks/e2-symqg-fastscan-verdict.md)); o spike off-PG chegou a
> 1,8–2,66× vs a referência ([`e2-symqg-spike.md`](../benchmarks/e2-symqg-spike.md)), mas isso **não
> transferiu** para dentro do PostgreSQL. **Use `theodb_hnsw` como default vetorial**; o `theodb_symqg` é um
> AM **alternativo/experimental** de pesquisa. Nenhuma promessa de superioridade de latência é feita aqui.

Esta página cobre a criação do índice vetorial experimental `theodb_symqg` no TheoDB — o access method,
a opclass (somente L2), a reloption `degree_bound`, o knob de recall de scan e o kill-switch do kernel
FastScan — sempre com o veredito medido honesto de que o `theodb_hnsw` é o caminho vetorial recomendado.

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` (own-code), que provê o tipo `vector` e registra o AM `theodb_symqg`
(`theodb_rs/src/am/mod.rs:87`).

---

# 2. Criar índice `theodb_symqg` básico (somente L2)

```sql
CREATE INDEX itens_symqg_idx
ON itens
USING theodb_symqg (
    embedding theodb_symqg_l2_ops
);
```

Cria o índice de grafo quantizado co-localizado. A **única** opclass é `theodb_symqg_l2_ops` (default, distância
Euclidiana `<->`) — o estimador de sinal 1-bit é L2-only, então não há opclass de cosseno/produto interno
(`theodb_rs/src/am/mod.rs:359-360`). Um build com opclass não-L2 falha rápido
(`theodb_rs/src/am/build.rs:456-459`).

---

# 3. Definir o `degree_bound` (grau de saída por vértice)

```sql
CREATE INDEX itens_symqg_r64
ON itens
USING theodb_symqg (
    embedding theodb_symqg_l2_ops
)
WITH (
    degree_bound = 64
);
```

`WITH (degree_bound = R)` define o grau de saída por vértice do grafo co-localizado. O valor é **arredondado
para cima para um múltiplo de 32** (alinhamento do FastScan) e limitado ao intervalo válido; ausente,
o default é **32** (`theodb_rs/src/am/options.rs:199` + `degree_bound_from_relation`,
`theodb_rs/src/am/options.rs:458-466`). `R` maior ⇒ grafo mais denso + linhas maiores.

---

# 4. Consulta por vizinho mais próximo (L2 `<->`)

```sql
SELECT *
FROM itens
ORDER BY embedding <-> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Busca os 10 vizinhos mais próximos por distância Euclidiana. Só o operador `<->` é suportado (opclass L2-only);
`<=>` (cosseno) e `<#>` (produto interno) **não** têm opclass neste AM.

---

# 5. Ajustar recall/velocidade do scan (`ef_search`)

```sql
SET theodb_hnsw.ef_search = 80;   -- pool de candidatos por hop; default 64
```

O scan do `theodb_symqg` faz beam search sobre um grafo-base HNSW e honra o mesmo knob de recall
`theodb_hnsw.ef_search` (`theodb_rs/src/am/scan.rs:246` usa `guc::ef_search()`; default 64,
`theodb_rs/src/am/guc.rs:24`). `ef` maior ⇒ mais recall, menos QPS — foi o eixo medido no verdict (ef=80 → recall ~0,95).

---

# 6. Kill-switch do kernel FastScan 1-bit (A/B — default ON)

```sql
SET theodb.symqg_fastscan = off;   -- força o baseline escalar (A/B); default = on
```

Controla se o scan usa o kernel FastScan 1-bit batched (SIMD) ou o estimador escalar de sinal. Default **ON**
(`theodb_rs/src/am/guc.rs:121` — `SYMQG_FASTSCAN = GucSetting::new(true)`; registro em `:336`). É um
kill-switch de A/B mesmo-índice; o ganho medido do kernel é modesto (1,07–1,22×,
[`e2-symqg-fastscan-verdict.md`](../benchmarks/e2-symqg-fastscan-verdict.md)).

---

# 7. Comparar com o índice vetorial recomendado (`theodb_hnsw`)

```sql
-- default vetorial recomendado (mais rapido in-PG no recall casado):
CREATE INDEX itens_hnsw_idx
ON itens
USING theodb_hnsw (
    embedding theodb_hnsw_l2_ops
);
```

Para produção vetorial, o `theodb_hnsw` é o caminho recomendado: medido **2,6–3,9× mais rápido** que o
`theodb_symqg` in-PG no recall casado em SIFT1M
([`e2-symqg-inpg-verdict.md`](../benchmarks/e2-symqg-inpg-verdict.md)). O `theodb_symqg` existe como AM
alternativo/experimental.

---

# 8. Persistência e crash-safety (VACUUM / restart)

```sql
VACUUM itens;
```

Como os demais AMs vetoriais own-code do TheoDB, o `theodb_symqg` persiste o grafo em páginas WAL-logadas
(build → restart → scan retorna resultados idênticos) e trata INSERT via região pending + rebuild no VACUUM.
A corretude de recall/pending/VACUUM/MVCC foi verificada no verdict in-PG
([`e2-symqg-inpg-verdict.md`](../benchmarks/e2-symqg-inpg-verdict.md)).

---

# 9. Fluxo completo (experimental)

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE INDEX itens_symqg_idx
ON itens
USING theodb_symqg (
    embedding theodb_symqg_l2_ops
)
WITH (
    degree_bound = 32
);

SET theodb_hnsw.ef_search = 80;   -- recall/velocidade do scan

SELECT id, embedding <-> '[0.12,0.45,0.81]'::vector AS dist
FROM itens
ORDER BY dist
LIMIT 10;
```

Fluxo completo (para experimentação/pesquisa):

1. instala a extensão `theodb` (registra o AM `theodb_symqg`);
2. cria o índice `theodb_symqg` (opclass L2-only, `degree_bound` opcional);
3. ajusta `theodb_hnsw.ef_search` para o trade-off recall/velocidade;
4. consulta por `<->` (L2).

> **Lembrete honesto:** o veredito medido é que o `theodb_hnsw` supera o `theodb_symqg` in-PG
> (2,6–3,9× mais rápido no recall casado). Prefira `theodb_hnsw` como default vetorial; o `theodb_symqg`
> é um AM alternativo/experimental e **não** promete superioridade de latência.
