---
type: Technique
title: O canário mínimo separa 'nosso código está quebrado' de 'a plataforma não roda' em segundos
description: 30+ jobs de CI morriam em 2-3 s com zero steps; um workflow de um único echo também falhou — falsificando a hipótese do repositório e provando que o bloqueio era upstream.
resource: .claude/knowledge-base/discoveries/blueprints/ci-restore-signal-blueprint.md
tags: [ci, diagnostico, experimento, metodo]
timestamp: 2026-07-30T00:00:00Z
---

# O canário mínimo separa "nosso código quebrou" de "a plataforma não roda"

## O caso (#140)

**30+ jobs consecutivos** no `develop` falhando, cada um morrendo em **2–3 segundos com zero steps executados**.
As releases v0.113.0–v0.118.0 foram todas mergeadas vermelhas; a verificação do programa M127–M132 inteiro veio de
corridas medidas no droplet, não do CI.

Duas hipóteses concorrentes, indistinguíveis pelo sintoma:

- **(A)** nossos workflows estão quebrados;
- **(B)** o Actions não roda para esta conta, ponto.

**O experimento decisivo** foi o menor possível: um workflow sem dependências, sem secrets, sem services, com um
único `echo`.

```
job: canary   started 15:50:42Z   completed 15:50:44Z   conclusion: failure
steps: 0                       # nenhum step chegou a começar
logs: zip de 22 bytes (vazio)  # o runner não produziu saída alguma
```

> Um job de um único `echo` em `runs-on: ubuntu-latest` **não pode falhar por razão do repositório**.
> **Hipótese A falsificada.**

Reportado como **BLOCKED**, não contornado (Regra 3).

## A técnica

1. Quando um sintoma admite "nós" ou "eles", construa o **menor artefato que ainda exercita a plataforma** — um
   `echo`, um `SELECT 1`, um `GET /health`.
2. Se o mínimo falha, **a causa não está no seu código** — e nenhuma quantidade de leitura do seu código vai
   encontrá-la. Pare de procurar ali.
3. Os sinais de "morreu antes de começar" são específicos e valem mais que a `conclusion`: **zero steps**,
   **duração de segundos**, **log vazio**, campo de runner **em branco**.
4. **Reporte BLOCKED**, com o canário como evidência. Contornar um bloqueio de plataforma produz um workaround
   permanente para um problema temporário.

## Precedente idêntico noutra plataforma

O runbook de migração para a Blacksmith registra o mesmo formato de sinal — job **`queued` com `runner_name`
vazio por mais de 24 h** — e a mesma lição: a causa era uma pré-condição de **conta** (repo tem de pertencer a uma
organização; o App tem de estar instalado **nela**), invisível em qualquer leitura do YAML. O wizard oficial
respondia "No changes to migrate" com zero runners servindo.

## Relacionados

- [technique/controle-positivo](controle-positivo.md) — o irmão: provar que o instrumento morde
- [failure-mode/diagnostico-aceito-sem-reproduzir](../failure-modes/diagnostico-aceito-sem-reproduzir.md)
- [technique/separar-transporte-de-conteudo](separar-transporte-de-conteudo.md)
