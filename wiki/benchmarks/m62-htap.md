---
type: Measurement
title: m62 — superfície HTAP em três eixos, e a restrição que a redesenhou
description: ~31× de ganho analítico sem degradar o OLTP, ao preço de refresh explícito; e o achado medido de que a dependência proíbe execução dentro de funções.
resource: git:f7c7b93:docs/benchmarks/m62-htap.md
tags: [benchmark, htap, codegen, oltp, freshness, m62]
milestone: M62
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m62
    resource: git:f7c7b93:docs/benchmarks/m62-htap.md
    title: M62 — superfície HTAP unificada
---

**Veredito:** o HTAP funciona e entrega **~31× a 5M** **sem degradar o OLTP**, ao preço de um refresh
explícito (~1,2 s) e de freshness datada.

# O achado arquitetural — medido, não suposto

Chamar o motor analítico **dentro** de uma função dispara erro, e **não há parâmetro que permita**.

Portanto a superfície **não é uma chamada única transparente**: é um fluxo em que **as funções geram o
SQL e o cliente o executa na conexão**. É a aposta de lakehouse **assistida**, não o colunar in-memory
auto-mantido da referência de mercado.

**Uma restrição da dependência, descoberta por medição, redesenhou a superfície** — e o
[ADR 0021](/decisions/0021-m62-htap-codegen-surface.md) argumenta que o resultado é o desenho **correto**
dada a restrição, não um workaround.

# Os três eixos

1. **Ganho analítico:** ~31×, com checksum casado — correção verificada, não assumida.
2. **Não-interferência:** o p95 de OLTP **não degrada** sob carga analítica concorrente, porque o
   snapshot é somente-leitura. **Medir a interferência, e não só o ganho**, é o que torna a afirmação de
   HTAP defensável.
3. **Custo:** refresh explícito, freshness datada e **storage 2×**.

# Ressalva de comparabilidade

O ~31× é de uma agregação com agrupamento; a varredura completa do
[m61](/benchmarks/m61-columnar-adoption.md) deu ~9×. **São queries diferentes**, e o documento diz isso —
sem o qual alguém compararia os dois números como se medissem a mesma coisa.

# O que aconteceu depois

A restrição que forçou o codegen **desapareceu** quando a dependência foi removida, e o desenho
**colapsou** para funções que fazem o trabalho internamente
([ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)).
