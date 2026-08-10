---
scenario: theo-rag-sobre-theodb
date: 2026-08-10
operator: claude-code/opus-5
outcome: pass
summary: A imagem foi publicada em ghcr.io pela primeira vez na história do projeto, e o compose do theo-rag sobe healthy contra ela com o schema real aplicado.
---

# O que foi feito

`ghcr.io/usetheoai/theo-db:latest` e `:0.140.0` **publicadas** — a primeira vez que a imagem existe no
registry. Até hoje `docker pull ghcr.io/usetheoai/theo-db:latest`, o primeiro comando do README, respondia
`manifest unknown`.

# O bloqueador que ninguém sabia que existia

O workflow `publish-image.yml` referenciava `usetheodev/theo/.github/workflows/build-publish.yml` — **org que
não existe**; o correto é `usetheoai`. Medido:

```
run em develop      → failure
run na tag v0.158.0 → failure
erro: workflow was not found
```

**Toda** execução falhava. Corrigido em `45312ee`.

E a consequência que mais importa: `ghcr.io/usetheodev/theo-db:0.139.0` — a tag que o PR
[theo-rag#206](https://github.com/usetheoai/theo-rag/pull/206) referenciava — **não existia**. Verificado com
`docker manifest inspect`. **O PR nunca poderia ter funcionado para quem o mergeasse**: o compose falharia no
pull, antes de qualquer coisa.

Terceiro defeito achado por uso na mesma sessão, e ele corrige um diagnóstico meu: eu havia atribuído o
bloqueio do B-010 a um gate humano de release. A medição mostrou que **o gate nunca chegava a ser alcançado**.

# Verificação da imagem publicada

Pull limpo de fora, contêiner novo, cenário do `theo-rag`:

| | |
|---|---|
| `docker pull ghcr.io/usetheoai/theo-db:latest` | funciona |
| planner a 20k × `vector(1536)` | **`Index Scan`** (era `Seq Scan`, 182 ms) |
| BM25 no binário default | `bm25_build`, `bm25_search` |
| compose do `theo-rag` contra a imagem | **`Up (healthy)`** |
| schema de produção via `drizzle-kit push` | **24 tabelas** |

O PR foi atualizado para `ghcr.io/usetheoai/theo-db:0.140.0` (`theo-rag@2996bed3`).

# Um achado operacional, auto-infligido

A publicação ficou 12+ minutos na fila porque o runner self-hosted é **único e serial**, e estava ocupado
pelo workflow `Rust test suite` que eu havia criado horas antes (B-013), com timeout de 45 min. Cancelei o
meu para liberar a fila.

**O `rust-suite.yml` precisa não monopolizar o runner compartilhado** — hoje qualquer push que toque
`theodb_rs/**` bloqueia publish, lint e CI por até 45 minutos. Registrado como trabalho pendente.

# O que isto NÃO prova

O âncora exige o `theo-rag` **servindo consultas reais na infraestrutura que o time opera**. Isto é a imagem
publicada e o compose de desenvolvimento subindo — o caminho está inteiro e verificado, e o merge do PR mais
o deploy continuam sendo decisão do time.
