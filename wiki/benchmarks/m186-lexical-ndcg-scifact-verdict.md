---
type: Measurement
title: m186 — a busca lexical own-code entrega 2,08× o nDCG@10 do ts_rank_cd nativo em corpus público
description: Primeira medição de qualidade de recuperação do pilar lexical, contra o BEIR SciFact e contra o baseline que o usuário já tem no Postgres — com significância pareada e o limite de método que provavelmente subestima o nosso número.
resource: benchmarks/artifacts/m186/lexical-ndcg-scifact.json
tags: [benchmark, m186, lexical, bm25, ndcg, beir, significancia, qualidade-de-recuperacao]
milestone: M186
generated: { by: claude-code/opus-5, at: 2026-08-09T20:00:00Z }
sources:
  - id: ndcg
    resource: benchmarks/artifacts/m186/lexical-ndcg-scifact.json
    title: nDCG@10 sobre 300 consultas do SciFact com julgamento humano
---

O pilar lexical entrou no binário default em 2026-08-09 **sem um único número de qualidade** — o
[m184](/benchmarks/m184-pilares-superficie-medida-verdict.md) havia registrado o eixo como estruturalmente
aberto. Este artefato fecha esse eixo.

# A medição

BEIR SciFact: 5 183 documentos, **300 consultas com julgamento humano** (`qrels/test.tsv`, score > 0).

| caminho | nDCG@10 |
|---|---|
| **`bm25_build` + `bm25_search` (nosso)** | **0,6269** |
| `to_tsvector('english')` + GIN + `ts_rank_cd` (o que o usuário já tem) | 0,3016 |

**Delta +0,3253**, IC 95% `[0,2811, 0,3677]`, bootstrap pareado com B = 10 000 e semente fixa 20260809:
**p < 0,0001**. Significativo, e por margem que não depende do teste escolhido.

O controle **não é ingênuo**: roda com stemming e stopwords do Postgres, sobre índice GIN. É a configuração
que um usuário montaria sem nós.

# Calibração — o número é plausível, não inflado

A referência publicada do BEIR para BM25/Anserini neste dataset é ≈ 0,665. **Cito isso de conhecimento
interno e não verifiquei nesta sessão** — a regra 8 manda declarar, e o acervo local não está no disco.
Sujeito a correção.

Sob essa ressalva: nosso 0,6269 fica **logo abaixo** de um BM25 maduro e afinado, que é exatamente onde uma
implementação própria honesta deveria cair. Um número *acima* da referência seria motivo de suspeita, não de
comemoração.

# O limite de método, que corta a nosso favor

**`bm25_search` aceita um termo por chamada.** Para uma consulta multi-termo eu somei os scores por termo —
aproximação grosseira do BM25 multi-termo real, que receberia a consulta inteira e normalizaria uma vez.

Isso quase certamente **subestima** o nosso número. O resultado favorável foi obtido *apesar* da agregação
grosseira, não graças a ela.

**E é um achado por si:** a superfície pública não expõe busca multi-termo, e é dela que qualquer usuário
real precisaria. Consertar isso é trabalho de produto, não de benchmark.

# Segundo corpus — a vantagem generaliza em direção, não em magnitude

O primeiro corte deste artefato media um dataset só, e eu registrei isso como limite. Fechado com um segundo
corpus de domínio diferente — **BEIR NFCorpus**, médico, 3 633 documentos, 80 consultas com julgamento:

| corpus | nosso | `ts_rank_cd` | razão | delta | p |
|---|---|---|---|---|---|
| SciFact (científico, n = 300) | 0,6269 | 0,3016 | **2,08×** | +0,3253 | < 0,0001 |
| NFCorpus (médico, n = 80) | 0,3138 | 0,2331 | **1,35×** | +0,0807 | < 0,0001 |

**A direção replica** — ambos significativos por bootstrap pareado, IC 95% inteiramente positivo nos dois.
**A magnitude não.** 2,08× contra 1,35× é diferença grande demais para ser ruído amostral.

**Consequência direta:** "mais que o dobro do `ts_rank_cd`" é propriedade **do SciFact**, não do produto.
A afirmação defensável é *"melhor que o `ts_rank_cd` nativo com significância pareada em dois corpora de
domínios distintos, por margem que varia com o domínio"*. Escrevi a primeira versão no CHANGELOG antes de ter
o segundo corpus; ela foi corrigida antes de virar release.

A calibração se mantém no segundo corpus: a referência BM25/Anserini para NFCorpus é ≈ 0,325 (conhecimento
interno, **não verificado**), e nosso 0,3138 fica logo abaixo — o mesmo padrão do SciFact, o que reforça que o
número não está inflado.

# O que este artefato NÃO mede

- **Dois datasets, ambos pequenos e biomédicos/científicos.** Dois pontos são melhores que um e continuam não
  sendo uma curva. Domínios distantes — jurídico, código, conversacional — seguem sem medição.
- **Latência.** Só qualidade. O [m184](/benchmarks/m184-pilares-superficie-medida-verdict.md) mediu o
  `tsvector` nativo em 30,1 ms sobre 20k; o nosso caminho não foi cronometrado aqui.
- **A fusão híbrida.** O [m123](/benchmarks/m123-hybrid-significance.md) mediu o ganho do híbrido sobre o
  vetorial puro como não-significativo. Um lexical melhor **não implica** fusão melhor — a armadilha central
  deste pilar é justamente essa, e continua aberta.
