---
type: Measurement
title: m177 fase 1 — a qualidade em pt-BR fecha o gate, e ela anda junto com a latência
description: O modelo mais rápido perde 37% de MRR para o melhor; a fronteira de Pareto tem três pontos e cinco dos oito candidatos são dominados, o que torna a escolha uma troca explícita e não um almoço grátis.
resource: benchmarks/artifacts/m177/quality-ptbr-knownitem.json
tags: [benchmark, m177, embedding, qualidade, multilingual, pt-br, known-item, pareto, licenca]
milestone: M177
dataset: wiki pt-BR (250 conceitos, known-item)
generated: { by: claude-code/opus-5, at: 2026-08-08T02:00:00Z }
sources:
  - id: quality
    resource: benchmarks/artifacts/m177/quality-ptbr-knownitem.json
    title: Qualidade e latência por modelo multilíngue, corpus pt-BR, CPU dedicada
---

Fecha o **único item da fase 1 do M177 que nunca teve número**. Tudo o que o milestone mediu até aqui era
custo — latência, memória, throughput. Um modelo barato que recupera mal não serve, e a escolha tinha
3,7× de diferença de latência sem nenhuma evidência de qualidade para pesar contra.

# O corpus, e por que ele é este

Não existe corpus pt-BR com qrels neste repositório, e inventar julgamento de relevância seria fabricar
evidência. A saída honesta foi usar um corpus **real do próprio projeto** com relevância **derivável**:

| | |
|---|---|
| documento | `title` + corpo do conceito da wiki (250 conceitos) |
| consulta | a `description` do frontmatter — uma frase que resume o conceito, **escrita à parte** |
| relevância | 1:1, o conceito de origem — *known-item retrieval* |

A relevância é ground-truth **por construção**, não um juízo meu. Instrumento:
`benchmarks/m177_quality_ptbr.py`, reusando `theodb_bench/knownitem.py` (`mrr_at_k`, `success_at_1`,
`recall_known_item`) em vez de reimplementar métrica.

# O resultado

Droplet `c-8` de CPU dedicada, `OMP_NUM_THREADS=1`, 250 consultas, uma por vez (o regime real da consulta):

| modelo | dim | **MRR@10** | S@1 | R@10 | latência p50 | Pareto |
|---|---|---|---|---|---|---|
| `intfloat/multilingual-e5-large` (MIT) | 1024 | **0,7906** | 0,704 | 0,936 | 108,0 ms | **✓** |
| `nomic-embed-text-v1` (apache-2.0) | 768 | **0,6749** | 0,548 | 0,908 | 53,7 ms | **✓** |
| `paraphrase-multilingual-MiniLM-L12-v2` (apache-2.0) | 384 | **0,4946** | 0,412 | 0,684 | **12,7 ms** | **✓** |
| `paraphrase-multilingual-mpnet-base-v2` | 768 | 0,4568 | 0,340 | 0,732 | 30,7 ms | dominado |
| `jina-clip-v1` | 768 | 0,4557 | 0,332 | 0,700 | 56,5 ms | dominado |
| `jina-embeddings-v2-base-de` | 768 | 0,4023 | 0,300 | 0,612 | 50,9 ms | dominado |
| `jina-embeddings-v2-base-code` | 768 | 0,3440 | 0,244 | 0,592 | 47,9 ms | dominado |
| `Qdrant/clip-ViT-B-32-text` | 512 | 0,1130 | 0,072 | 0,220 | 21,4 ms | dominado |

**A fronteira tem três pontos; cinco dos oito candidatos são dominados** — existe outro modelo mais
rápido *e* melhor. O caso mais limpo é o `mpnet`: 30,7 ms e MRR 0,4568, contra o `MiniLM` com 12,7 ms e
MRR 0,4946. **Mais lento e pior**, sem compensação. Escolher por reputação ou por tamanho teria errado
aqui, e a medição anterior deste milestone — que só olhava latência — chamou o `mpnet` de "3,3× mais
rápido que o e5-large" sem saber que ele recupera 42% pior.

# A conclusão que corrige a leitura anterior

Ao fechar a medição de custo, este milestone registrou que "escolher o modelo vale ~30× mais que
embarcá-lo", com a implicação de que o modelo rápido poderia bastar. **A qualidade desmente a parte
otimista dessa leitura:**

- `MiniLM` é **8,5× mais rápido** que o `e5-large` — e perde **37% de MRR** (0,4946 contra 0,7906).
- `nomic-v1` é o meio-termo real: **metade da latência** do `e5-large` por **85% do MRR**.

**Qualidade e latência andam juntas neste corpus** — a ordem por MRR é quase a ordem inversa por
latência. Não há modelo que seja simultaneamente rápido e ótimo, o que torna a escolha uma **troca
declarada**, não uma otimização.

**Recomendação medida:** `nomic-embed-text-v1` como default do caminho de consulta (Apache-2.0, 768d,
53,7 ms, MRR 0,6749) e `multilingual-e5-large` quando a qualidade domina o requisito. O `MiniLM` só se a
latência for restrição dura — perder 37% de MRR é caro para 41 ms.

# Limites honestos — leia antes de citar qualquer número acima

- **Os valores absolutos são otimistas.** `description` e corpo do mesmo conceito compartilham
  vocabulário e autoria, o que facilita a recuperação. O que este artefato mede com validade é a
  **ordem entre modelos**, que é a decisão em jogo — não o MRR que o produto teria com consultas de
  usuário real.
- **Known-item não é busca semântica geral.** Cada consulta tem exatamente um documento certo. Um
  corpus com relevância graduada pode reordenar os modelos.
- **O corpus é técnico e do próprio domínio** — pt-BR de engenharia de banco de dados, com termos em
  inglês misturados. Não representa português coloquial nem outro domínio.
- **n = 250 consultas, sem teste de significância entre modelos.** As diferenças no topo (0,79 contra
  0,67) são largas o bastante para a ordem ser confiável; as do meio do ranking (0,4568 contra 0,4557)
  **não são distinguíveis** e não devem ser lidas como ordenação.
- **Dois modelos não foram medidos** por falha de download/carregamento (`nomic-embed-text-v1.5` e sua
  variante quantizada `-Q`), e ficam registrados como erro em vez de omitidos. A variante `-Q` é
  justamente a que testaria quantização — o eixo continua sem número.
- **Non-commercial não foi medido** (`jina-embeddings-v3`, `jina-reranker-v2-multilingual`): medir o que
  o D1 não deixa distribuir produz número que seduz e não pode ser usado.

# Relacionados

- O custo dos mesmos modelos: [m177 fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md)
- Onde a alavanca do modelo apareceu: [camadas](/benchmarks/m177-camadas-python-http-verdict.md)
- Capacidade por instância: [m177 stress](/benchmarks/m177-stress-colapso-verdict.md)
- A restrição de deploy: [CloudNativePG](/references/embedding-em-cloudnativepg-2026-08.md)
- O desenho atual: [embeddings em SQL](/guides/sql-embeddings.md)
