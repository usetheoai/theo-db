---
item: B-059
repo: theodb-bench
mode: review
date: 2026-08-17
verdict: pending
measured_on: droplet theo-b059-bench · 138.197.22.192 · s-8vcpu-16gb · nyc3 · ubuntu-24.04
image: google/alloydbomni:latest (digest pulled 2026-08-17)
---

# B-059 — o adapter do Omni, e a armadilha do concorrente medida no produto do concorrente

Todas as 12 medições rodaram no droplet efêmero **`138.197.22.192`**, nunca na máquina do owner.

## Corner 1 — Evidence

### M1 · A imagem está numa major atrás de nós, e o artigo tinha razão

```
PostgreSQL 17.9 on x86_64-pc-linux-gnu, compiled by Debian clang version 12.0.1
```

O TheoDB é **PG 18**. Uma corrida `theodb × alloydbomni` **cruza uma major do PostgreSQL** — e o avaliador
independente registrou exatamente esta divergência (imagem do Docker Hub em 17 enquanto os pacotes Linux já
estavam em 18). Não é motivo para não medir; é motivo para o bundle **declarar** o que mediu.

### M2/M3 · O que a imagem traz, e o que exige instalar

| Extensão | Versão | Instalada por default? |
|---|---|---|
| `alloydb_scann` | 0.1.4 | **não** |
| `vector` | **0.8.2.google-1** — fork Google do pgvector | **não** (vem por `CASCADE`) |
| `google_columnar_engine` | 1.0 | **sim** |
| `google_ml_integration` | 1.6 | sim |

`CREATE EXTENSION alloydb_scann CASCADE` → `NOTICE: installing required extension "vector"`.

Consequência que não estava óbvia: **a mesma imagem serve o pgvector**, porque o `vector` do Omni é um fork do
pgvector 0.8.2 e traz `hnsw`/`ivfflat`. Isso é uma oportunidade de comparação com PG idêntico — e uma armadilha,
porque *não* é o pgvector upstream.

### M4 · Os opclasses do `scann` não seguem a convenção do pgvector — e isso quebra a tabela atual

```
scann :: cosine       | tipo=vector
scann :: dot_product  | tipo=vector
scann :: l2           | tipo=vector
```

Contra `postgres.py:66`, que é o que o arnês tem hoje:

```python
OPCLASSES = {
    "hnsw":    {"l2": "vector_l2_ops", "ip": "vector_ip_ops", "cosine": "vector_cosine_ops"},
    "ivfflat": {...},
}
```

**Nenhum nome coincide.** O `scann` chama `cosine` o que o pgvector chama `vector_cosine_ops`, e `dot_product`
o que ele chama `vector_ip_ops`. Um adapter que herdasse a tabela emitiria
`USING scann (emb vector_cosine_ops)` e falharia — ou, pior, um AM futuro aceitaria o nome errado.

O Omni também registra um AM **`ivf`** próprio (distinto do `ivfflat` do pgvector), com a convenção
`vector_*_ops`. Duas convenções coexistindo na mesma instalação.

### M5/M6/M7 · A armadilha central do artigo, reproduzida — e o portão do [[B-060]] a pega

Sessão nova, sem `LOAD`:

```
select count(*) from pg_settings where name like 'scann%';        →  1
SET scann.num_leaves_to_search = 500;                            →  SET      (sucede)
select ... from pg_settings where name='scann.num_leaves_to_search'; → AUSENTE DO pg_settings
current_setting('scann.num_leaves_to_search', true);             →  500      (ecoa o placeholder)
```

Depois de `LOAD 'alloydb_scann'` na mesma sessão:

```
select count(*) from pg_settings where name like 'scann%';        → 111
SET scann.num_leaves_to_search = 500;                            → setting=500 src=session
```

E `shared_preload_libraries` = `g_stats, google_columnar_engine, google_job_scheduler,
google_ml_integration, google_storage` — **`alloydb_scann` não está lá**, então o `LOAD` é exigido por sessão.

**Isto é a falha do B-060 acontecendo no produto do concorrente.** `current_setting` devolve 500; o motor busca
no default `0`. O gate que o B-060 entregou — que lê `pg_settings`, não `current_setting` — **recusa** a corrida.
A ordem de dependência que escolhi (B-060 antes de B-059) está agora justificada por medição, não por argumento:
sem o portão, a primeira corrida contra o ScaNN publicaria um default como resultado.

### M8/M9 · `quantizer` é string, e o renderizador atual só sabe inteiro

```
CREATE INDEX ... USING scann (emb cosine) WITH (num_leaves=10, quantizer='sq8')   → CREATE INDEX
reloptions gravadas: num_leaves=10 · quantizer=sq8
WITH (nao_existe=99)                                                             → ERROR: unrecognized parameter
```

Contra `postgres.py`, `index_ddl`:

```python
rendered = ", ".join(f"{key} = {int(value)}" for key, value in sorted(parameters.items()))
```

`int('sq8')` levanta `ValueError` — **não** `AdapterError` com contexto. O review do B-060 previu isto
textualmente (§ R-6: *"o risco R1 continua real para um motor futuro cujo GUC seja booleano ou enum — o ScaNN
traz `quantizer='sq8'`"*) e deixou para quando um knob exigisse. Este item é esse knob.

O lado honesto: o Omni **recusa** reloption inválida (M9). Esse eixo não tem o defeito de "aceitar em silêncio".

### M10 · O quantizador que dá velocidade ao ScaNN vem DESLIGADO

```
scann.enable_ah_quantizer   = off
scann.num_leaves_to_search  = 0
scann.pct_leaves_to_search  = 0
scann.num_search_threads    = 2
```

O AH (asymmetric hashing) é o mecanismo que o `wiki/decisions/0035-m73-northstar-vector-verdict.md` cita como
a razão do gap de 25-44× — e no Omni ele é **opt-in**. Um `theodb × scann` medido no default mediria o ScaNN
sem o que o torna ScaNN. Fato material para o [[B-057]], registrado aqui porque foi medido aqui.

### M11/M12 · O plano nomeia o índice, e numa tabela pequena não o usa

```
200 linhas, default              →  Seq Scan on probe
200 linhas, enable_seqscan=off    →  Index Scan using probe_ok on probe
```

`assert_index_used` (`postgres.py:325`) casa pelo **nome do índice no plano** e funciona sem mudança para este
AM. E o Seq Scan no default confirma por que ele existe: sem ele, uma corrida pequena reportaria "performance
do ScaNN" tendo medido varredura sequencial.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `src/adapters/alloydb.py` (novo) | `AlloyDBOmniAdapter`; herda `PgvectorAdapter` porque o tipo `vector` e os operadores `<=>`/`<->`/`<#>` são os mesmos (fork do pgvector) |
| `src/adapters/postgres.py` | `index_ddl` precisa renderizar reloption não-inteira; `OPCLASSES` deixa de ser tabela global e passa a ser declarada pela subclasse |
| `src/registry.py:74` | uma entrada `alloydbomni` |
| `src/adapters/base.py` | **nada** — o contrato do B-060 já cobre o efetivo |
| Bundles publicados | **nenhum invalidado** — sistema novo, não altera número existente |
| Schemas versionados (11) | **nenhum bump** — `system.json` já carrega `version` livre e `points[].parameters` é objeto aberto |
| [[B-057]] | **depende disto** e ganha M10: medir no default mediria o ScaNN sem AH |
| [[B-058]] | ganha M2/M12 — o `google_columnar_engine` **já vem instalado e pré-carregado**, então "Omni off" exige desligar, não deixar de instalar |

## Corner 4 — Verification

1. `theodb-bench doctor` reporta `alloydbomni` como construível, ou diz o que falta — sem adivinhação.
2. `capabilities()` declara **só** o que este código exercita, e não sugere storage desagregado, read pool ou
   failover gerenciado — que o Omni não tem (é query layer).
3. A versão vai ao bundle **lida do servidor** (`select version()` + `extversion`), nunca inferida da tag —
   provado por um teste que casa contra o que o servidor respondeu.
4. Uma reloption string (`quantizer='sq8'`) é renderizada, e uma inválida levanta `AdapterError` com contexto,
   não `ValueError` cru.
5. O adapter emite `LOAD 'alloydb_scann'`, e o portão do B-060 **prova** que o knob entrou em vigor — com um
   teste que falha se o `LOAD` for removido.
6. Uma corrida `theodb × alloydbomni × pgvector` na mesma máquina produz bundle válido.

## Reclassificação

`suggested_mode: review` → mantido em espírito, mas a evidência é **runtime medido** (12 execuções contra o
produto real), não leitura de código. O bloco do item registra a divergência.
