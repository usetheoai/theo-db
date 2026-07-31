# M169 — delta medido: 28/43 → 30/43 (+2)

Medição de **conclusão**, não de velocidade: o critério é *a consulta termina* sob o mesmo teto.

## O que ficou constante — sem isto a subtração não significa nada

| | antes (T1.2) | depois (T4.1) |
|---|---|---|
| `so_md5` | `a6ab650771f00b5a0d66af2220709168` | `5ba1e09efa3dcc41d78f5124a604f278` |
| `nproc` / `mem_gb` | 16 / 31 GB | 16 / 31 GB |
| `data_directory` | `/srv/m169data` | `/srv/m169data` |
| linhas em `hits` | 99997497 | 99997497 |
| `statement_timeout` | 300 s | 300 s |
| `work_mem` | 256MB | 256MB |

O `so_md5` é a ÚNICA linha que muda, e é essa a variável independente.

## Ganhos, separados por atribuição

- **4 atribuíveis a este milestone** — falhavam COM roteamento agregado e agora completam: q20, q32, q33, q34
- **0 NÃO atribuíveis** — não falhavam no caminho agregado, então este fix não é a explicação: (nenhuma)

## REGRESSÕES — completavam antes e falham agora

A linha mais importante do documento, e a que um resumo de 'quantas a mais passam?' esconde.

| q | veredito depois | `agg_routed` | erro |
|---|---|---|---|
| q08 | `error:XX000` | True | df_executor: datafusion: Execution error: (Hint: you may |
| q09 | `error:XX000` | True | df_executor: datafusion: Execution error: (Hint: you may |

## Ainda falhando NO caminho agregado

q08, q09 — o que resta no caminho que este milestone toca.


---

## Adendo honesto (acrescentado no /review, 2026-07-31)

Três coisas que a tabela acima **não** diz e que mudam como ela deve ser lida.

**1. Este delta descreve o binário `5ba1e09e`, que ainda tinha a regressão.** As duas linhas de q08/q09 sob
"REGRESSÕES" foram corrigidas depois (ADR-0059) e remedidas com o binário `debde5f3`: q08 `ok` 28,5 s, q09 `ok`
36,6 s, q32 `ok` 295,6 s. Um artefato que não descreve o binário entregue não serve como evidência dele — a
corrida completa com o binário final é o que substitui este documento.

**2. "30/43" não é ganho homogêneo.** Duas das consultas que completam o fazem pelo **recuo ao caminho eager**,
com o consumo O(N) que este milestone existe para remover — provado por duas linhas de
`theodb_agg_stream_fallback` no log do servidor, uma por consulta. A leitura correta é
**"28 pelo streaming + 2 pelo recuo"**, e o harness hoje **não** distingue os dois: `agg_routed` vem do
`EXPLAIN`, que é fato de planejamento e é idêntico nos dois braços (achado do review).

**3. A q32 passa com 1,5% de margem.** 295,6 s contra um teto de 300 s. Numa corrida marginalmente mais
carregada ela vira `timeout`. "Passa" ali é frágil, não é folgado.

### Orçamento de descritores — sem isto o número não é reproduzível

| | |
|---|---|
| `ulimit -n` (soft, do postmaster) | 1024 |
| `max_files_per_process` | 1000 |

A regressão de q08/q09 foi **`EMFILE`**, com 205 GB de disco livres: não é memória nem disco. Quem repetir esta
corrida noutra caixa com outro orçamento de descritores pode não reproduzir nem a regressão nem o recuo.
