---
scenario: theo-rag-sobre-theodb
date: 2026-08-10
operator: claude-code/opus-5
outcome: partial
summary: A tentativa de migrar o theo-rag encontrou um defeito que 109 benchmarks não pegaram — o planner rejeitava todo índice vetorial.
---

# O que foi feito

Verificação de drop-in do `theo-rag` sobre o TheoDB: schema real do produto (`chunks` com `vector(1536)`),
`CREATE EXTENSION vector` pelo shim, `CREATE INDEX ... USING hnsw`, inserts e consultas — **sem alterar uma
linha do `theo-rag`**. PR aberto em `usetheoai/theo-rag#206`.

# O defeito, e por que ele conta como "achado por uso"

Ao fechar a última verificação em aberto — teste de escala com 20 000 linhas — o planner escolheu
`Seq Scan` (182,117 ms) tendo um índice capaz de responder em 1,994 ms. Custo estimado do índice: **94×
maior** quando era **91× mais rápido**.

**Nenhum dos 109 artefatos de benchmark do projeto detectou isto**, e a razão é estrutural: todo benchmark
força o caminho que quer medir (`SET enable_seqscan=off` ou equivalente). Só quem chega pelo caminho de um
usuário — criar índice e consultar — encontra.

Este é exatamente o critério que o DoD do B-010 chama de "ao menos um defeito encontrado por uso, não por
benchmark". Ele foi satisfeito **por uma tentativa de dogfood, não pelo dogfood em regime** — a distinção
está registrada abaixo.

Correção medida e verificada: `wiki/benchmarks/m175-planner-cost-inversion-verdict.md`. Depois dela, mesmo
cenário e mesmas páginas: startup 134,21, `Index Scan`, **6,401 ms**.

# O limite honesto desta evidência

**Isto não é uso sustentado.** É uma verificação de compatibilidade conduzida por um agente, num contêiner
efêmero, com dados sintéticos. O âncora exige o `theo-rag` **servindo consultas reais na infraestrutura que o
time opera**, e isso não aconteceu.

O que esta evidência prova: o drop-in funciona, e a tentativa já produziu o tipo de achado que benchmark não
produz. O que ela **não** prova: que o produto aguenta carga real, dados reais e falhas reais ao longo do
tempo — que é a única coisa que move o status para `running`.
