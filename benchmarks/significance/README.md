# Significância pareada

O que responde à pergunta que todo artefato de benchmark do projeto declarava não responder: **a diferença
medida sobrevive ao acaso?**

| Módulo | Papel |
|---|---|
| `significance.py` | permutação pareada (Smucker/Allan/Carterette, CIKM 2007) + bootstrap pareado + t como verificação cruzada. **Recuperado byte a byte** de `7cd157d^`, numpy puro |
| `per_query.py` | `PerQueryEvaluator` — dirige o laço de consultas sobre a porta `VectorDB.search_documents` e computa a métrica com as funções do próprio arnês |
| `compare.py` | alinha por `qid`, compara N sistemas par a par, persiste os arrays |
| `run_lexical_significance.py` | aplica tudo aos motores reais e **verifica que a média reproduz o agregado publicado** |

## Por que o dado vem por fora do arnês

O VectorDBBench computa métrica por consulta e a descarta — `runner/serial_runner.py:238-240` monta os
arrays e o método devolve só as médias. Persistí-los lá exigiria mudar a tupla de retorno, todos os
chamadores e o dataclass `Metric`, atravessando o núcleo que a Política de Fork manda não tocar.

Reusar a porta dá o mesmo dado **e** uma garantia extra: todos os sistemas veem as mesmas consultas na
mesma ordem, que é o pré-requisito do teste pareado.

## A verificação que torna o `p` confiável

`run_lexical_significance.py` compara a média por consulta com o agregado que a corrida publicou. Tolerância
`5e-4` — metade da última casa do `round(..., 4)` que o `serial_runner` aplica. **Se divergir, nada é
publicado**, e a tolerância é constante: afrouxá-la por motor seria ajustar o gate para caber no resultado.

## Um `p` alto não é uma coisa só

`verdict()` distingue os dois casos, porque tratá-los como iguais é como se afirma paridade sem tê-la medido:

- `p` alto com **IC estreito** em torno de zero → evidência de equivalência
- `p` alto com **IC largo** → falta de poder, e **não** evidência de equivalência

## Rodar

```bash
python -m pytest benchmarks/significance -q          # 22 testes, sem banco e sem rede
python benchmarks/significance/run_lexical_significance.py \
    --published benchmarks/vectordbbench/results-lexical/json \
    --out benchmarks/significance/per-query/b047.json
```
