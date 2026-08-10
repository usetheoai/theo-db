---
type: Technology
title: Go
description: A linguagem designada para a camada de produto e operação no mandato de código próprio — e deliberadamente NÃO para extensões in-engine.
resource: https://go.dev/
tags: [tecnologia, linguagem, control-plane, escopo]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: go-site
    resource: https://go.dev/
    title: Go, site oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

Go é uma linguagem compilada com coleta de lixe e concorrência leve, dominante em ferramental de
infraestrutura e serviços de rede.[^recalled]

# Papel neste acervo — definido por exclusão

O [ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) designou Go para a **camada de produto e
operação** — operador Kubernetes, control plane, CLI, gateway — e **Rust** para as camadas *in-engine*.

**A parte interessante é a rejeição registrada:**

> **Go para extensões in-engine** — rejeitada: extensões PostgreSQL de hot-path **não se escrevem em
> Go**; o ferramental de extensão é Rust, e C é a alternativa. **Go fica no control plane, seu lugar
> idiomático.**

O motivo é técnico e concreto: uma extensão in-engine roda **dentro do processo do PostgreSQL**, sob o
gerenciamento de memória e o modelo de erro dele — territórios em que uma linguagem com runtime próprio e
coletor de lixo não se encaixa sem atrito severo.

**Escolher a linguagem pelo lugar em que o código roda**, e registrar a escolha rejeitada com a razão, é
o que impede que "usamos Go" vire justificativa para usá-lo onde não cabe.

# Fora do escopo deste acervo

O código Go do ecossistema — operador, control plane — **não vive neste repositório**. Como o
[CLAUDE.md do projeto](/decisions/0006-own-code-postgres-based-rust-go.md) registra, **alta
disponibilidade, replicação e control plane são preocupações de deploy e plataforma**; este repositório é
**o banco**: engine mais extensão.

Por isso este acervo tem conceitos de Rust, pgrx e PostgreSQL em profundidade, e nenhum de Go — a
fronteira do repositório é também a fronteira do conhecimento aqui registrado.

[^go-site]: Go, site oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
