---
type: Measurement
title: m147 — refactor do scan byte-idêntico e neutro em QPS
description: Três eixos de limpeza estrutural — máquina de versão, fronteira de erro única e kernel compartilhado — com o decode por versão deliberadamente mantido separado.
resource: git:f7c7b93:docs/benchmarks/m147-ab-byte-identical.md
tags: [benchmark, refactor, byte-identico, ocp, tratamento-de-erro, m147]
milestone: M147
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m147
    resource: git:f7c7b93:docs/benchmarks/m147-ab-byte-identical.md
    title: M147 — refactor de am/scan.rs byte-idêntico
    last_modified: 2026-07-24
---

Refactor **preservador de comportamento**, provado por A/B byte-idêntico e neutro em throughput.

# Os três eixos

1. **Uma escada de cinco predicados de versão, cada um relendo o bloco de metadados**, virou **um enum
   lido uma vez** com casamento exaustivo. Além de ler menos, o casamento exaustivo faz o compilador
   **exigir** tratamento de uma versão nova — a diferença entre uma escada que silenciosamente cai no
   `else` e uma que não compila.
2. **Cerca de 56 blocos de tratamento de erro no estilo C** viraram **uma única fronteira de erro** com
   propagação. Menos superfície onde um erro pode ser tratado errado, e um só lugar onde ele vira erro do
   banco.
3. **O kernel de pontuação, copiado byte a byte em cinco corpos**, virou uma função compartilhada que
   **recebe o offset do chamador**.

# A restrição respeitada

O terceiro item vem com uma linha que vale ler: **o decode on-disk por versão permanece separado**.

Ou seja, **compartilhou-se o cálculo e não o parsing de formato** — porque unificar o decode de versões
diferentes de formato é exatamente onde um refactor "que não muda nada" quebra compatibilidade com dados
já gravados. **O limite do refactor foi decidido por uma decisão de arquitetura anterior, e respeitado.**

# A metodologia

O A/B segue o padrão estabelecido: mesmo índice físico, binários pré e pós, resultados comparados por
identidade — o mesmo rigor de [m126](/benchmarks/m126-hnsw-split-byteidentical.md).
