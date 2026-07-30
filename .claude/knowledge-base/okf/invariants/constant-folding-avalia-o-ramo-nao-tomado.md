---
type: Invariant
title: O PostgreSQL dobra expressões constantes no planejamento — `CASE ... ELSE 1/0` erra SEMPRE
description: Um gate escrito como CASE com divisão por zero constante dispara mesmo quando o ramo não é tomado, e o sintoma se lê como "o fix não funcionou".
resource: benchmarks/m169_agg_stream.sql
tags: [postgres, planner, teste, gate]
timestamp: 2026-07-30T00:00:00Z
---

# O PostgreSQL dobra expressões **constantes** no planejamento

## O invariante

`CASE` é curto-circuito para **subexpressões que dependem de dados**. Não é garantia para **constantes**: o
planejador dobra `1/0` durante o planejamento, e o erro sai antes de o `WHEN` ser avaliado.

```sql
-- ARMADILHA: dispara SEMPRE, inclusive quando a condição é verdadeira
SELECT CASE WHEN cond THEN 1 ELSE 1/0 END;
```

## Por que a direção do erro é a cara

Um gate escrito assim **reprova sempre** — antes do fix e depois dele. E o sintoma que o operador vê é
*"o gate ainda está vermelho, então o fix não funcionou"*, que manda caçar um defeito inexistente no código que
acabou de ser corrigido.

É a forma **invertida** de [teste-que-passa-pela-razao-errada](../failure-modes/teste-que-passa-pela-razao-errada.md):
um teste que **reprova** pela razão errada é igualmente inútil, e mais caro, porque consome depuração.

## A forma que este projeto usa

Bloco `DO` com `RAISE EXCEPTION` (`benchmarks/m168_pending_rows.sql:80-104`). Sem armadilha de folding, acumula
**todas** as falhas numa mensagem só em vez de parar na primeira, e diz o que falhou:

```sql
DO $gate$
DECLARE bad text := '';
BEGIN
  IF <condição ruim> THEN bad := bad || 'o que falhou e por quê. '; END IF;
  IF bad <> '' THEN RAISE EXCEPTION 'GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'GATE ok: <o que ficou provado>';
END
$gate$;
```

Se por algum motivo for preciso forçar erro numa expressão, o denominador tem de **depender de dados** para não
ser dobrado — por exemplo dividir por `(<predicado volátil>)::int`.

## Relacionados

- [failure-mode/teste-que-passa-pela-razao-errada](../failure-modes/teste-que-passa-pela-razao-errada.md)
- [technique/controle-positivo](../techniques/controle-positivo.md) — o braço de auto-teste que prova que o gate morde
- [technique/gate-de-nao-vacuidade](../techniques/gate-de-nao-vacuidade.md)
