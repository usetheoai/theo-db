---
type: Technique
title: Um oráculo só é confiável se ele reprova um caso deliberadamente errado
description: Antes de confiar num verificador, prove que ele CONSEGUE reprovar — senão 'zero divergências' pode significar 'não olhou'.
resource: benchmarks/columnar_type_ab.py
tags: [oraculo, teste, gate]
timestamp: 2026-07-30T00:00:00Z
---

# Um oráculo só é confiável se ele reprova um caso deliberadamente errado

## O padrão

Todo oráculo carrega, no mesmo arquivo, um caso **construído para falhar**. Se ele passa, o oráculo está quebrado
e a corrida **aborta** antes de produzir qualquer número.

```python
def test_type_ab_positive_control_reports_divergence():
    GIVEN um par deliberadamente divergente no EDGE_CATALOG
    WHEN o oráculo roda sobre ele
    THEN assert result.diverged > 0   # se der 0, o oráculo está quebrado
```

## Onde provou valor

`benchmarks/columnar_type_ab.py` (M163) nasceu porque o A/B do ClickBench, usado como oráculo de correção,
**não exercita o espaço de tipos** — bugs de classe de tipo (widening de inteiro, temporal, float IEEE, colação)
sobreviviam a ele e só caíam no review do conselho, depois de 14 min de rebuild. O M161 sozinho teve 1 BLOCKER e
1 HIGH que o A/B `int4-int4` nunca dispararia.

O controle positivo é o que torna esse oráculo diferente: ele **prova que pegaria** o defeito que motivou sua
criação.

## Regra derivada de cobertura

Um oráculo com catálogo precisa de guard contra apodrecimento: `test_edge_catalog_has_all_routed_types` falha
quando um tipo roteado não tem casos de borda. Sem isso, o catálogo envelhece em silêncio.

## Relacionados

- [technique/gate-de-nao-vacuidade](gate-de-nao-vacuidade.md)
- [failure-mode/gate-desligado-em-silencio](../failure-modes/gate-desligado-em-silencio.md)
