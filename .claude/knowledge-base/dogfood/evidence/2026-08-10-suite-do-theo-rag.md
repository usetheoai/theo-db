---
scenario: theo-rag-sobre-theodb
date: 2026-08-10
operator: claude-code/opus-5
outcome: partial
summary: A suíte do theo-rag rodou contra o TheoDB — 197 passam — e o caminho até lá revelou um bloqueador que invalidava a alegação "sem alterar uma linha" do próprio PR.
---

# O que foi feito

O `docker-compose.yaml` do `theo-rag` apontado para uma imagem TheoDB com todas as correções do dia, e a
**suíte de integração real do repositório** executada contra ela. Não é benchmark sintético: é o que o
produto roda.

# O bloqueador — segundo defeito achado por uso

O contêiner entrou em **loop de reinício**, com volume existente **e com volume limpo**:

```
Error: in 18+, these Docker images are configured to store database data in a
       format which is compatible with "pg_ctlcluster" ...
       Counter to that, there appears to be PostgreSQL data in:
         /var/lib/postgresql/data (unused mount/volume)
```

**O PostgreSQL 18 mudou a convenção do diretório de dados** — subdiretórios por versão sob
`/var/lib/postgresql` — e a imagem **recusa iniciar** se encontra um mount em `.../data`, que é o caminho
correto para PG ≤ 17 e o que todo compose existente usa.

Corrigido (`pgdata:/var/lib/postgresql`) e verificado: `Up (healthy)`, `accepting connections`. A correção
foi enviada ao próprio PR (`usetheoai/theo-rag@da42597c`).

**Isto invalidava a alegação central do meu PR.** Eu havia escrito *"sem alterar uma linha do `theo-rag`"* —
falso, e só apareceu ao rodar a suíte de verdade. A verificação anterior, num contêiner sintético que eu
mesmo configurei, nunca poderia ter encontrado: eu montava o volume do jeito certo sem saber que era um jeito.

**Alcance real:** qualquer usuário migrando de um Postgres pré-18 para o TheoDB encontra isto, e a mensagem
de erro não menciona o TheoDB — parece defeito da migração dele.

# A suíte

```
Test Files  2 failed | 24 passed | 26 skipped (52)
Tests       3 failed | 197 passed | 122 skipped (322)
```

**As 3 falhas não são do banco**, e verifiquei uma a uma: duas leem `ROADMAP-v8.md`, um arquivo de
documentação ausente naquele repositório; a terceira parou em `THEORAG_PG_URI` não exportada e depois em
schema não migrado — `db:push` exige TTY e meu shell não tem.

**197 testes de integração do `theo-rag` passam contra o TheoDB.**

# O limite honesto

Isto é a suíte do produto, não o produto servindo usuários. O status do âncora **permanece `planned`**: não
houve carga real, dados reais, nem operação ao longo do tempo. O que esta evidência acrescenta à anterior é
que o caminho até o uso real ficou mais curto e um bloqueador a menos — e que o segundo defeito do dia
também veio de uso, não de medição.
