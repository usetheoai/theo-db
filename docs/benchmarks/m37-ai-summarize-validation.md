# M37 — Validação funcional: `ai.summarize` / `ai.agg_summarize`

**Data:** 2026-07-03
**Tipo:** validação de contrato de superfície de IA (não é throughput-benchmark — ver Nota de honestidade).
**Container sob teste:** `theo-db:m36` (extensão `theodb_rs`), porta 5452, `--add-host=host.docker.internal:host-gateway`.

## Contexto (honesto)

O M37 foi aberto sob a premissa "`ai.summarize` não implementado". O grounding measurement-first
**falsificou a premissa**: a feature já estava entregue (M10) e testada. A auditoria de features que
gerou o M37 foi incompleta — grepou só o Rust (`theodb_rs/src/`) e perdeu a implementação em
`sql/50-theodb-ai.sql`. M37 virou correção de doc-drift + esta validação de evidência, **sem código novo**
(um `ai.summarize` em Rust seria duplicado / conflito de install).

## Superfície entregue (verificada ao vivo)

| Objeto | Assinatura (introspecção `pg_proc`) | Fonte |
|---|---|---|
| `ai.summarize` | `(content text, model text DEFAULT NULL) -> text` | `sql/50-theodb-ai.sql:32` (plpgsql → `ai._chat` Rust) |
| `ai.agg_summarize` | `agg_summarize(text)` (agregado) | `sql/50-theodb-ai.sql:82` (`_agg_summ_accum`/`_agg_summ_final`) |

Segurança (least-privilege, ADR `docs/adr/0007-synchronous-per-row-model-http.md`): `proacl` de ambas =
`{postgres=X/postgres}` → **REVOKE FROM PUBLIC** aplicado (só o owner executa). Provado por
`test_ai_functions_not_executable_by_public`.

## Evidência funcional

### 1. Suíte de contrato offline (stub determinístico) — 33 passed, 3 skipped

```
$ PGPORT=5452 python3 -m pytest tests/test_ai_sql.py -v
======================== 33 passed, 3 skipped in 2.05s =========================
```

Cobre summarize escalar + agregado, caminhos de erro tipado (empty completion, null input, propagação
de erro), volatilidade do finalfunc, skip de linhas null/empty no agregado.

### 2. Real-OpenAI ao vivo (gpt-4o-mini) — 3 passed

```
$ THEODB_LLM_ENDPOINT=https://api.openai.com/v1/chat/completions \
  THEODB_LLM_MODEL=gpt-4o-mini OPENAI_API_KEY=… \
  python3 -m pytest tests/test_ai_sql.py -k real_openai -v
tests/test_ai_sql.py::test_real_openai_sentiment_polarity     PASSED
tests/test_ai_sql.py::test_real_openai_agg_summarize_shape    PASSED   # <- summarize agregado real
tests/test_ai_sql.py::test_real_openai_generate_batch_shape   PASSED
====================== 3 passed, 33 deselected in 16.79s =======================
```

### 3. Exemplo escalar ao vivo (gpt-4o-mini real)

Entrada (1 parágrafo sobre o TheoDB) → saída:

> "TheoDB is an open-source database that is compatible with PostgreSQL and replicates AlloyDB using
> permissively licensed open-source components, focusing on vector-search performance and built on top
> of PostgreSQL with mature extensions."

Resumo coeso de 1 frase — contrato honrado.

## Nota de honestidade (Regra 3 + `public-copy.md`)

- **Não há throughput-benchmark de sumarização.** A latência/qualidade de `ai.summarize` são dominadas
  pelo LLM configurado (modelo síncrono por-linha, ADR 0007), não pelo TheoDB. Um número de "resumos/s"
  seria uma medida do provedor de LLM, não da nossa engine — publicá-lo como claim do TheoDB seria
  fabricação. A validação correta desta superfície é o **teste de contrato contra o container real**
  (acima), não um QPS.
- A superfície entregue é `ai.summarize` / `ai.agg_summarize` no schema `ai`. As seções de "versionamento
  `theodb_ml` / flags de preview / cursor" nas páginas de feature descrevem a API-alvo estilo AlloyDB, não
  a superfície entregue — marcado explicitamente nos docs.

## Reprodução

```bash
docker run -d --name theodb-m37 -p 5452:5432 \
  --add-host=host.docker.internal:host-gateway -e POSTGRES_PASSWORD=postgres theo-db:m36
docker exec theodb-m37 psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;"
cd benchmarks
PGPORT=5452 python3 -m pytest tests/test_ai_sql.py -v                 # 33 passed, 3 skipped
THEODB_LLM_ENDPOINT=https://api.openai.com/v1/chat/completions \
  OPENAI_API_KEY=$OPENAI_API_KEY \
  PGPORT=5452 python3 -m pytest tests/test_ai_sql.py -k real_openai -v  # 3 passed
```
