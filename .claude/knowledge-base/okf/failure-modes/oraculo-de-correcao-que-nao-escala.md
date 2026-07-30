---
type: Failure Mode
title: O oráculo de correção é O(N) e morre antes do sistema
description: O guard que prova correção puxa o resultado inteiro para o cliente; na escala alvo ele estoura antes de qualquer conclusão sobre o produto.
tags: [oraculo, escala, harness]
timestamp: 2026-07-30T00:00:00Z
---

# O oráculo de correção é O(N) e morre antes do sistema

## Assinatura

O harness morre, não o produto. E como os dois morrem por OOM, a causa é atribuída ao produto.

## Caso pago — M169

`run_m128_clickbench.py:283` remove o `LIMIT` **de propósito** — por razão correta: com empates, o corte do
`LIMIT 10` é arbitrário, então comparar a agregação completa é o oráculo honesto. Mas depois faz `fetchall()`:

```python
ab_sql = re.sub(r"\s+LIMIT\s+\d+\s*;?\s*$", "", sql...)
cur.execute(ab_sql); rc = _canonical(cur.fetchall())
```

A 1M isso é inofensivo. A 100M, com `GROUP BY UserID, m, SearchPhrase`:

| Processo | `anon-rss` | Origem |
|---|---|---|
| `postgres` | **12,3 GB** | agregação sem `LIMIT` → todos os grupos materializados no backend |
| `python3` | **32,2 GB** | `fetchall()` dos mesmos grupos no cliente |

E eu quase filei isso como regressão do pushdown. A medição isolada mostrou **4,58 GB** com pushdown ON e 4,57 GB
com OFF — o `LIMIT 10` limita a materialização; a versão sem limite não limita nenhum dos dois lados.

## Como evitar

- Oráculo de correção precisa ser **limitado** por construção: cursor server-side, hash do resultado, ou
  agregação de segunda ordem (`count`, `sum`, checksum) em vez de materializar linhas.
- Quando cliente e servidor morrem juntos, **isole**: rode a consulta como o usuário a escreveu antes de acusar
  o motor.

## Relacionados

- [measurement/q17-pushdown-nao-e-regressao](../measurements/q17-pushdown-nao-e-regressao.md)
- [failure-mode/diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md)
