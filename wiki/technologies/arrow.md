---
type: Technology
title: Apache Arrow
description: O formato colunar em memória que é a linguagem comum entre o PostgreSQL e o executor analítico — e a fronteira onde as conversões de tipo precisam ser exatas.
resource: https://arrow.apache.org/
tags: [tecnologia, columnar, memoria, formato, interoperabilidade]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: arrow-site
    resource: https://arrow.apache.org/
    title: Apache Arrow, site oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O Arrow é um **formato colunar em memória** padronizado, com implementações em várias linguagens e
licença permissiva. Ele define como arrays tipados, valores nulos e estruturas aninhadas ficam dispostos
na memória, de modo que sistemas diferentes possam operar sobre os mesmos buffers **sem serialização**.[^recalled]

# Papel neste acervo

**É a linguagem comum** entre o storage colunar do projeto e o
[executor vetorizado](/technologies/datafusion.md): os dados são decodificados das páginas para arrays
Arrow, processados, e convertidos de volta para o formato do PostgreSQL.

**As três conversões dessa fronteira são onde mora o risco**, e cada uma tem artefato:

- **decodificar para Arrow** — o caminho cujo custo por célula foi o gargalo que
  [m160](/benchmarks/m160-decode-zerocopy-verdict.md) removeu, passando a construir uma alocação tipada
  por coluna em vez de uma por célula;
- **construir arrays tipados corretamente** — inclusive temporais, que exigiram mapeamento explícito de
  domínio em [zone-map temporal](/benchmarks/columnar-zonemap-temporal-verdict.md);
- **converter de volta** — a parte não trivial da cola, porque o resultado precisa voltar com **os tipos
  exatos** do PostgreSQL, sob pena de violar a garantia de identidade byte a byte.

# Onde mais ele aparece

Os **codecs de compressão** do formato colunar em disco vêm do ecossistema Arrow, decisão registrada no
[ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md) — que optou por **framing próprio com codecs
reusados**, mantendo a crash-safety nativa do Postgres e não reinventando compressão.

E o [cache Arrow](/benchmarks/archive/m101-arrow-cache.md) mantém lotes pré-construídos em memória, com
invalidação MVCC-correta.

[^arrow-site]: Apache Arrow, site oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
