---
type: Measurement
title: b096 — o custo do read_parquet são DOIS round-trips por texto, e só um deles é nosso
description: Medido no arnês, seis escalas × quatro consultas: remover a travessia Arrow→NDJSON→Value dá 1,085× mediano, não a ordem de grandeza estimada. O segundo round-trip vive no pgrx (JsonB→texto→jsonb_in) e é inescapável enquanto o retorno for jsonb — o que confirma a hipótese do item e refuta a minha leitura dela.
tags: [colunar, parquet, jsonb, pgrx, b-096, honest-negative, retratacao]
item: B-096
generated: { by: claude-code/opus-5, at: 2026-08-22T03:00:00Z }
---

Peças: [b058 — o crossover do colunar](b058-crossover-colunar.md), onde o Parquet apareceu 40× a 142×
mais lento que o heap; [runbook do droplet](../runbooks/droplet-de-medicao.md).

# A pergunta

O [[B-096]] afirmava: *"enquanto a assinatura for `SETOF jsonb`, nenhuma otimização de parsing muda a
ordem de grandeza"*. A DoD pedia separar parsing de materialização e medir um protótipo tipado no
mesmo sweep.

# A decomposição, e a conclusão errada que tirei dela

Medido na máquina de desenvolvimento, 2M linhas, mesmo arquivo:

| | tempo |
|---|---|
| parser Parquet + agregação no DataFusion (`olap()`) | **25 ms** |
| piso: 2M linhas por SRF no Postgres | 315 ms |
| piso: 2M `jsonb` construídos nativamente pelo PG | 435 ms |
| `read_parquet` → 2M `jsonb` | **4 650 ms** |

O parser não era o gargalo, e `jsonb` em si também não. **Daí eu concluí que os ~4 200 ms restantes
eram a travessia `Arrow → texto NDJSON → serde_json::Value`, e que a assinatura não era o problema —
declarando a hipótese do item "parcialmente refutada".** Escrevi isso no `CHANGELOG` antes de medir o
conserto.

# O conserto, medido no arnês

Conversão direta Arrow → `serde_json::Value`, sem o NDJSON. Duas imagens construídas do mesmo commit
vizinho, no **mesmo droplet e na mesma hora**, benchmark registrado `analytical/crossover/row-count`,
perfil `nightly`, veredito **`VALID`** nas duas.

| consulta | 10K | 50K | 100K | 500K | 1M | 2M |
|---|---|---|---|---|---|---|
| `total_rows` | 1,07× | 1,08× | 0,98× | 1,12× | 1,08× | 1,02× |
| `sum_amount` | 1,14× | 1,08× | 0,99× | 1,10× | 1,09× | 1,02× |
| `group_by_category` | 1,12× | 1,09× | 1,00× | 1,09× | 1,10× | 1,01× |
| `filtered_sum` | 1,21× | 1,09× | 1,00× | 1,09× | 1,10× | 1,01× |

**Razão mediana 1,085× (n=24; mín 0,98×, máx 1,21×).** ~8,5%, não a ordem de grandeza que eu havia
estimado a partir da decomposição.

# Por que: havia DOIS round-trips, e removi um

A discrepância não é ruído — é atribuição errada da minha parte. Dentro do `pgrx` 0.19.0,
`src/datum/json.rs:136`:

```rust
impl IntoDatum for JsonB {
    fn into_datum(self) -> Option<pg_sys::Datum> {
        let string = serde_json::to_string(&self.0).unwrap();   // Value → TEXTO
        // …
        direct_function_call_as_datum(pg_sys::jsonb_in, …)      // TEXTO → jsonb binário
    }
}
```

**Cada linha ainda é serializada para texto e re-parseada pelo `jsonb_in` do Postgres.** Eu removi o
primeiro round-trip (Arrow → NDJSON → `Value`); o segundo é do `pgrx` e não há como escapar dele
enquanto a função devolver `jsonb`.

**Isso confirma a hipótese do [[B-096]] e refuta a minha leitura dela.** O item dizia que a assinatura
era o teto; eu disse que não era. O item estava certo, e por uma razão mais precisa do que a que ele
próprio enunciava — não é "a assinatura" em abstrato, é que a ponte `pgrx`→PG para `jsonb` passa por
texto, por linha.

# O que isto decide

- **A correção fica**, porque é estritamente menos trabalho por linha e os testes de equivalência
  provam que o documento entregue não muda. Mas 8,5% **não** torna o Parquet um caminho de consulta
  quente: a 2M linhas ele segue em ~6 s contra ~0,06 s do heap.
- **O terceiro bullet da DoD é o que se aplica:** o limite fica declarado, e o Parquet não deve ser
  comparado como caminho de consulta quente em QPS por consulta. A pergunta que o item deixou aberta —
  *se o uso pretendido é varredura em lote, caso em que a métrica está errada e não o código* — segue
  aberta e agora tem lastro para ser respondida.
- **O caminho para ordem de grandeza é uma interface TIPADA**, que evita `jsonb` inteiramente. O
  `olap()` já demonstra o padrão: mesmo arquivo, mesmo DataFusion, **25 ms**.

# Ressalvas declaradas

- Veredito `nightly`, não `release`: `cpu_governor` não é exposto numa VM
  ([runbook](../runbooks/droplet-de-medicao.md)). Nenhum número daqui é `publishable`.
- O commit `antes` (`0405526`) também não tem a correção de estimativa do [[B-097]]. Isso afeta o
  caminho **colunar**, não o **Parquet** — e é o Parquet que esta medição lê.
- A decomposição da primeira seção é de **máquina de desenvolvimento**, com variância alta na baseline
  (4 409 a 7 076 ms em cinco repetições). Ela serve para localizar o custo, não para quantificá-lo; o
  número publicado é o do arnês.

# Artefatos

- antes: `benchmarks/artifacts/b096/antes/20260822T002000Z-analytical-crossover-row-count-theodb-d276194d/`
- depois: `benchmarks/artifacts/b096/depois/20260822T003027Z-analytical-crossover-row-count-theodb-7cbd4d78/`
- smoke: `benchmarks/artifacts/b096/smoke/20260822T001921Z-analytical-synthetic-paths-theodb-726cf1e1/`

# Reprodução

```bash
SNAPSHOT=theo-bench-base TAGS="antes:0405526 depois:HEAD" \
SUITE=analytical/crossover/row-count PROFILE=nightly CPU_SET="0-11" MEM_MAX="48G" \
  theodb-bench/ops/bench-droplet.sh
```
