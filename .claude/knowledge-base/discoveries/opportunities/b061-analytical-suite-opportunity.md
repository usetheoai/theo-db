---
item: B-061
repo: theodb-bench
mode: evolve
date: 2026-08-17
verdict: pending
measured_on: droplet theo-b059-bench · 138.197.22.192 · s-8vcpu-16gb · nyc3
---

# B-061 — a suíte analítica não existe, e a prova de residência que o SOTA recomenda não prova residência

Todas as medições no droplet efêmero `138.197.22.192`.

## Corner 1 — Evidence

### A fundação existe inteira, e nada a alcança

| Peça | Estado medido |
|---|---|
| `src/bench/analytical.py` | **336 linhas**, completo: `AnalyticalWorkload`, `AnalyticalBenchmark`, `generate_rows`, `expected_answer` (oráculo próprio), `compare_paths` |
| `AnalyticalTable` / `AnalyticalQuery` / `AnalyticalResult` | definidos em `adapters/base.py` |
| Três caminhos declarados | `row`, `columnar`, `parquet`, com capability por caminho |
| Quatro queries | `total_rows`, `sum_amount`, `group_by_category`, `filtered_sum` |
| `load_analytical` / `execute_analytical` | implementados **só** no `FakeAdapter` (`fake.py:666,682`) — **nenhum** adapter PostgreSQL |
| `BENCHMARKS` em `registry.py` | **duas** entradas, ambas vetoriais |

O andaime está pronto e não há motor ligado a ele. É por isso que o modo é `evolve` e não `bug`: nada está
quebrado, falta a peça que torna o resto alcançável.

### O colunar do TheoDB é armazenamento; o do Omni é cache — e isso muda o que "residência" significa

```
TheoDB  : select amname,amtype from pg_am → heap(t), theodb_columnar(t)
          CREATE TABLE c_probe(...) USING theodb_columnar;  INSERT 50 000 → OK
          select count(*),sum(amount) from c_probe → 50000 | -1879.18   (correto)
          pg_class.relam = theodb_columnar                              ← residência ESTRUTURAL
```

No TheoDB o colunar é um **table access method**: a tabela *é* colunar por construção, e `pg_class.relam` prova
isso sem ambiguidade. Não há como estar "ligado e vazio".

No Omni é um **cache populado por política**, e é exactamente aí que mora a falha.

### A falha do Omni, reproduzida em quatro estados distintos

| # | Estado | `g_columnar_columns` | `Memory Used` | Plano |
|---|---|---|---|---|
| 1 | `enabled = off` (**default**) | **erro**: *"module must be loaded via shared_preload_libraries and google_columnar_engine.enabled must be turned on"* | — | `Seq Scan` |
| 2 | `enabled = on`, store não populado | **0** | 0 MB | `Seq Scan` |
| 3 | `enabled = on`, `google_columnar_engine_add()` chamado, **`/dev/shm` = 64 MB** (default do Docker) | **4** | **0 MB** | `Seq Scan` |
| 4 | idem, `--shm-size=4g` | 43 | **42 MB** | **`Parallel Custom Scan (columnar scan)`** |

**O estado 3 é o achado, e ele é mais afiado que o do avaliador independente.** Ele recomenda
`g_columnar_columns` como a prova de residência. Medido: a view reporta **4 colunas** enquanto
`g_columnar_engine_summary` reporta **`Memory Used = 0 MB`**. Ela reporta **registro, não residência**.

Um portão construído sobre ela passaria com o store vazio — e a corrida mediria `Seq Scan` sob o nome do
colunar do AlloyDB, sem erro nem aviso. **É a mesma forma do `current_setting` vs `pg_settings` do [[B-060]]:
o instrumento óbvio reporta o pedido, não o efeito.**

E o estado 3 acontece por default em Docker: `google_columnar_engine_refresh` falha com
`could not resize shared memory segment … No space left on device (errno=28)` + `HINT: You may need to increase
the shared memory for the container`. Sem `--shm-size`, o store **nunca** carrega.

### Os instrumentos corretos, medidos

| Pergunta | Instrumento |
|---|---|
| o engine está ligado? | `show google_columnar_engine.enabled` — **`context=postmaster`**, exige **restart**, não é `SET` de sessão |
| as colunas estão registradas? | `g_columnar_columns` |
| o store está **carregado**? | `g_columnar_engine_summary` → `Memory Used (MB) > 0` |
| o plano **usou** o colunar? | `Custom Scan (columnar scan)` no `EXPLAIN` |
| residência do nosso lado? | `pg_class.relam = theodb_columnar` |

O plano do estado 4, na íntegra, mostra ainda o mecanismo de fallback:

```
Parallel Append
  ->  Parallel Custom Scan (columnar scan) on big_probe
        Rows Removed by Columnar Filter: 510157
        Rows Aggregated by Columnar Scan: 100609
        Columnar cache search mode: native
  ->  Parallel Seq Scan on big_probe (never executed)
```

O `Seq Scan` irmão sob `Parallel Append` é o caminho para o que **não** está residente. Residência parcial
portanto é observável no plano — e uma corrida que só olhasse o topo do plano não a veria.

### O que o Omni escolhe a 50 000 e a 2 000 000 de linhas

Com engine ligado, store **registrado mas vazio** (estado 3): `Seq Scan` nas duas escalas. Com store carregado
(estado 4) a 2M: `Custom Scan (columnar scan)`. A 50 000, o artigo mede o colunar **perdendo** para o heap
(31,6 ms contra 27,5 ms a 100K) — o crossover do nosso lado é o que o DoD pede e ainda não foi medido.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `src/adapters/postgres.py` | `load_analytical` / `execute_analytical` para o caminho `row` (heap) |
| `src/adapters/postgres.py` (TheoDB) | caminho `columnar` via `USING theodb_columnar` + portão por `relam` |
| `src/adapters/alloydb.py` | caminho `columnar` via `google_columnar_engine_add` + portão por `Memory Used` **e** plano |
| `src/registry.py` | entrada de suíte analítica |
| `src/bench/analytical.py` | **provavelmente nada** — o andaime já cobre o que a suíte precisa |
| Schemas versionados | `result`/`statistics` já são genéricos; a confirmar ao escrever |
| [[B-058]] | **depende disto** e ganha os quatro estados: "Omni off" é `ALTER SYSTEM` + **restart**, não flag de sessão, e exige `--shm-size` ou mede store vazio |
| [[B-063]] | o portão colunar é o mesmo padrão apply-then-verify que lá está morto — aqui ele nasce **com chamador** |

## Corner 4 — Verification

1. Uma suíte analítica registrada roda contra `theodb`, `pgvector`/`postgres` e `alloydbomni`, produzindo bundle
   válido.
2. O portão de residência **recusa** cada um dos estados 1, 2 e 3 do Omni, com mensagem que os distingue —
   "engine desligado", "store vazio" e "registrado mas não carregado" levam a ações diferentes.
3. O portão é provado por reprovação: um duplo que reporta colunas registradas e `Memory Used = 0` é recusado.
4. Do nosso lado, o portão confirma `relam = theodb_columnar` e recusa uma tabela heap apresentada como colunar.
5. O crossover do nosso colunar é medido: a partir de quantas linhas ele vence o heap.

## Escopo que este item NÃO fecha, e por quê

O DoD registrado pede **shape TPC-H** (bullet 1) e **contenção escrita×scan nos dois regimes** (bullet 4).

- **TPC-H exige esquema multi-tabela.** `AnalyticalTable` é de uma tabela (`name`, `columns`, `path`); a Q5 do
  TPC-H junta seis. Suportá-lo é redesenhar o contrato analítico, não registrar uma suíte — e é trabalho de
  tamanho comparável a todo o resto deste item.
- **Contenção nos dois regimes exige arnês concorrente** (escrita e scan simultâneos, com p95/p99), que não
  existe no repositório.

Ambos ficam **declarados e registrados**, não silenciosamente omitidos.
