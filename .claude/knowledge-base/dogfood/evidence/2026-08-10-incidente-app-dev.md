---
scenario: theo-rag-sobre-theodb
date: 2026-08-10
operator: claude-code/opus-5
outcome: fail
summary: O banco do theo-rag no app-dev está em loop de reinício — a imagem TheoDB 0.140.0 foi implantada com o compose antigo, e o mount do PG 18 derruba o contêiner.
---

# O incidente

No droplet do `app-dev` (`165.227.121.20`):

```
therag_pgvector   ghcr.io/usetheoai/theo-db:0.140.0   Restarting (1) 17 seconds ago
```

Log:

```
The suggested container configuration for 18+ is to place a single mount
at /var/lib/postgresql which will then place PostgreSQL data in a subdirectory
```

**É o defeito do mount do PG 18**, medido e corrigido hoje mesmo — mas o `app-dev` recebeu a **imagem nova**
com o **compose antigo** (`pgdata:/var/lib/postgresql/data`). O banco do `theo-rag` não sobe.

# Por que isto é a evidência que faltava

A `dogfood-golden-rule` exige *"failure stories present ≥ 1 — a dogfood without failures is theatre"*. Esta é
a primeira **falha em operação real**, não em bancada: um ambiente que o time usa, derrubado por uma migração
que eu ajudei a preparar.

E ela confirma, do jeito mais desconfortável possível, a razão de o item existir. Eu havia medido este
defeito, escrito a correção, e mesmo assim ele chegou ao `app-dev` — porque a correção foi para o
`docker-compose.yaml` do **`theo-rag`** ([#211](https://github.com/usetheoai/theo-rag/pull/211)), e o
`app-dev` é implantado por **outro caminho**, com sua própria cópia da configuração.

**A lição não é sobre o PG 18.** É que corrigir um arquivo num repositório não corrige as cópias dele em
outro lugar — e ninguém sabia quantas cópias havia porque ninguém tinha mapeado o caminho de implantação.
Eu mesmo afirmei, duas horas antes, que o `theo-rag` "não está no `app-dev`", lendo o compose errado.

# Ação imediata necessária

O `app-dev` está com o banco do `theo-rag` fora do ar. A correção é a mesma de um caractere:

```yaml
- pgdata:/var/lib/postgresql        # era /var/lib/postgresql/data
```

**Não apliquei.** Mexer na configuração de um ambiente que o time opera, sem saber por qual pipeline ele é
gerido, arriscaria conflitar com o próximo deploy e mascarar a causa. O owner decide se corrige na origem
(o repositório que implanta o `app-dev`) ou no droplet.

# O que isto move no âncora

| | |
|---|---|
| `theo-rag` no `app-dev` sobre TheoDB | **implantado** — a imagem está lá |
| servindo | **NÃO** — o contêiner não sobe |
| história de falha em operação | **✅ esta** |

O âncora **não** vai a `running`: `running` exige uso, e um contêiner em loop é o oposto. Mas o item deixou
de estar bloqueado em "ninguém tentou" e passou a estar bloqueado em "está quebrado, e sabemos exatamente
por quê" — que é uma posição incomparavelmente melhor.
