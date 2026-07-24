# Review — publicação da imagem no GHCR (#187)

**Data:** 2026-07-24 · **Branch:** develop · **Issue:** #187

## Verdict: **READY_TO_MERGE**

Sem BLOCKER. A mudança adiciona um workflow novo, isolado, que não altera nenhum caminho existente do CI
nem código de produto.

## O problema (medido)

```
$ docker pull ghcr.io/usetheodev/theo-db:latest
Error response from daemon: manifest unknown
```

`README.md:93-94` e `docs/quickstart.md:12` instruem exatamente esse comando — **o primeiro passo da
documentação falhava**. Causa: os 6 usos de `docker/build-push-action` em `ci.yml` (linhas 55, 166, 206,
232, 292, 350) usam `load: true` com tag local (`theo-db:ci` / `theo-db:dev`); nenhum tem `push: true` e
nenhuma tag aponta para `ghcr.io`. A imagem era construída para os testes e descartada.

## Decisões de design

| Decisão | Rationale | Alternativa rejeitada |
|---|---|---|
| Disparo por **tag semver** (`v*.*.*`) | O projeto já corta `vX.Y.Z` a cada release; 1 imagem = 1 release, rastreável ao PR e ao GitHub release | Publicar em todo push de `develop` — encheria o registry de artefatos efêmeros |
| `linux/amd64` apenas | O runner é amd64; emular arm64 via QEMU multiplicaria o tempo de build numa fila já disputada | Multi-arch agora — YAGNI até haver demanda real |
| `concurrency` **sem** cancelamento | Abortar um push de imagem pela metade deixa manifesto parcial no registry | `cancel-in-progress: true` como nos demais workflows |
| Guard de ref semver no step `meta` | Recusa publicar de um ref inesperado (fail-fast, Regra 8) | Confiar no filtro de `on.push.tags` apenas |
| `permissions: packages: write` explícito | Menor privilégio; o token não recebe escopo além do necessário | Herdar permissões default do repo |

**Parsimony ladder:** rung 3 — `docker/login-action` + `build-push-action` são o mecanismo nativo da
plataforma; nada reimplementado (Regra 9). O build reusa contexto e cache do `ci.yml`, então a imagem
publicada é a mesma que os testes exercitaram.

## Gate de verdade (a lição de #181/#182)

O workflow não considera "publicado" o retorno 0 do push. Ele **puxa a imagem recém-publicada, sobe o
banco e executa o fluxo do README**: `CREATE EXTENSION vector` → tabela com coluna `vector(3)` →
`CREATE INDEX ... USING hnsw (e vector_cosine_ops)` → consulta de distância. Se qualquer passo falhar, o
job falha. É o caminho do usuário, não o do build — exatamente o que faltava e produziu #181, #182 e #187.

## Limite honesto desta entrega

O workflow **ainda não executou**: `workflow_dispatch` exige que o arquivo esteja no branch default
(`main`), e ele está em `develop`. A validação real acontece no próprio release — a tag `v*` gerada pelo
`cycle-release` dispara a publicação com o workflow já em `main`. **Até esse run concluir, o #187 não
está verificado**, e não deve ser fechado.

## Pendências registradas (não bloqueantes)

- **Visibilidade do package:** o GHCR cria packages como *private* por default. Para o `docker pull`
  anônimo do README funcionar, o package precisa ser tornado **público** nas settings — ação de owner,
  uma vez só, após a primeira publicação.
- Multi-arch (`linux/arm64`) quando houver demanda.

## Gates — verdes

YAML validado · nenhum caminho existente do CI alterado · sem secrets no workflow (usa
`secrets.GITHUB_TOKEN`) · sem commit em `main` · sem trailer de coautoria · CHANGELOG atualizado.

**Verdict:** READY_TO_MERGE
