# M169 — baseline ClickBench a 100M (baseline-100m)

**28/43 consultas completam.** Este é o número que o milestone existe para mover; ele é
uma medição de CONCLUSÃO, não de velocidade — o critério é *a consulta termina*.

## Proveniência

| | |
|---|---|
| `so_md5` | `a6ab650771f00b5a0d66af2220709168` |
| `nproc` | 16 |
| `free -g` (total) | 31 GB |
| `loadavg1` antes / depois | 1.1 / 1.0 |
| `data_directory` | `/srv/m169data` |
| `hits` (linhas, da tabela) | 99997497 |
| `hits_heap` | 0 |
| `statement_timeout` | 300 s |
| `work_mem` | 256MB |

O teto de 300 s é o do M162 — o `19/43` contra o qual este número se compara só é
comparável sob o MESMO teto.

## Vereditos

| veredito | n |
|---|---|
| `error:XX000` | 3 |
| `ok` | 28 |
| `timeout` | 12 |

**A/B columnar vs heap:** **n/a — nenhuma comparação columnar-vs-heap foi executada** (o gêmeo `hits_heap` estava ausente). Correção NÃO foi verificada nesta corrida.

## Falhas, separadas pelo discriminador `agg_routed`

Sem esta separação o número agregado é ambíguo: uma consulta que nem entra no caminho colunar
falha por razão que este milestone não endereça, e contá-la junto infla o alvo. `agg_routed` vem
do plano (`EXPLAIN`) via o sinal **agg-específico**, não do amplo `Custom Scan (theodb_columnar`
— que é quase sempre verdadeiro e esconde se o caminho AGREGADO roteou.

| q | veredito | `agg_routed` | erro |
|---|---|---|---|
| q17 | `timeout` | **False** | canceling statement due to statement timeout |
| q19 | `timeout` | **False** | canceling statement due to statement timeout |
| q20 | `error:XX000` | **True** | byte array offset overflow |
| q21 | `timeout` | **False** | canceling statement due to statement timeout |
| q22 | `timeout` | **False** | canceling statement due to statement timeout |
| q23 | `timeout` | **False** | canceling statement due to statement timeout |
| q24 | `timeout` | **False** | canceling statement due to statement timeout |
| q25 | `timeout` | **False** | canceling statement due to statement timeout |
| q26 | `timeout` | **False** | canceling statement due to statement timeout |
| q27 | `timeout` | **False** | canceling statement due to statement timeout |
| q28 | `timeout` | **False** | canceling statement due to statement timeout |
| q32 | `timeout` | **True** | canceling statement due to statement timeout |
| q33 | `error:XX000` | **True** | byte array offset overflow |
| q34 | `error:XX000` | **True** | byte array offset overflow |
| q39 | `timeout` | **False** | canceling statement due to statement timeout |

- **4 falhas COM roteamento agregado** — no caminho que o M169 toca: q20, q32, q33, q34
- **11 falhas SEM roteamento** — caem no executor de linha do PostgreSQL; fora do escopo declarado do plano, e nenhuma mudança no caminho colunar as move.

## O que este número NÃO autoriza a concluir (honestidade — Regra 3)

**O `19/43` do M162 não é base de comparação válida.** As duas corridas rodaram em regimes
diferentes de memória, não apenas em máquinas diferentes: a box do M162 tinha 15 GB e o corpus
de 16 GB era declaradamente *maior que a RAM*; esta tem 31 GB e o corpus **cabe em page cache**
(medido: 5 GB usados / 24 GB de cache). Uma diferença de contagem entre as duas corridas mistura
o efeito do código com o efeito do regime, e nenhuma das duas pode ser isolada *post hoc*.
O baseline honesto do M169 é ESTE número, medido nesta box; o delta que o milestone reivindicará
é T4.1 contra T1.2 — **mesma box, mesmo `so_md5` de dataset, mesmo teto**.

Consequência prática: consultas que falhavam no M162 e completam aqui **sem** `agg_routed` não
são evidência de melhoria de produto — são evidência de mais RAM. O discriminador acima existe
exatamente para impedir que essa atribuição seja feita por engano.

## GUCs efetivas

Lidas de volta de `pg_settings` após o `SET` — um parâmetro desconhecido no prefixo de uma extensão é
aceito como *placeholder* silencioso, então declarar a GUC pedida não prova que ela existe.

- `theodb.enable_columnar_agg` = `on`
- `theodb.enable_columnar_late_mat` = `on`
- `work_mem` = `262144`
- `max_parallel_workers_per_gather` = `0`
- `statement_timeout` = `300000`

## Reprodução

```bash
ALLOW_MISSING_HEAP=1 bash benchmarks/m169_baseline_100m.sh
python3 benchmarks/m169_baseline_summarize.py docs/benchmarks/m169-artifacts/baseline-100m.jsonl
```

