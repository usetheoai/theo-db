---
type: Measurement
title: m50 — régua SOTA entre os três índices, com caveats que precedem os números
description: Uma calibração de escala reduzida numa máquina suja, cuja honestidade está em separar o que é robusto ao ruído (ordenação relativa e recall) do que não é (latência absoluta).
resource: git:f7c7b93:docs/benchmarks/m50-sota-ruler.md
tags: [benchmark, regua, caveats, variancia, calibracao, m50]
milestone: M50
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m50
    resource: git:f7c7b93:docs/benchmarks/m50-sota-ruler.md
    title: M50 — Régua SOTA vetorial
    last_modified: 2026-07-06
---

**Veredito:** o índice próprio em paridade de recall com a referência, ~1,6–1,7× atrás em latência por
fator constante, e 29% menos QPS com 8 clientes no alto recall; o terceiro índice fica dominado nesta
escala in-memory.

# Os caveats vêm ANTES dos números — e é isso que torna o artefato utilizável

O documento abre com quatro ressalvas explícitas:

1. **Escala reduzida por decisão registrada.** O critério pedia um dataset realista dimensionado pela
   memória da máquina, mas ela tinha 12 containers ativos e o build materializa o corpus inteiro em RAM
   **sem teto**. O usuário escolheu rodar menor com caveats. O run realista fica **gated** no build em
   streaming ou numa máquina dedicada.
2. **A máquina NÃO estava quieta.** A carga subiu de 7,87 para até 12,64 numa máquina de 12 núcleos.
   **Consequência: os números ABSOLUTOS de latência e QPS carregam ruído de contenção externa.**
3. **O recall é confiável; a latência absoluta não.** O desvio de recall sobre 3 runs é ≤ 0,024 — e ≤
   0,007 no ponto que ancora o veredito. **Por isso o veredito se apoia em deltas relativos medidos no
   MESMO run e na MESMA máquina**, não em milissegundos absolutos.
4. **Um sub-item do critério NÃO foi medido** — a degradação de latência com pending acumulada — e fica
   registrado como follow-up honesto, **não como checkbox falso**.

# O que sobrevive ao ruído

**A ordenação relativa entre os três índices** — consistente com um cliente, com múltiplos clientes, e
nos três runs. É por isso que o veredito relativo se sustenta apesar da máquina suja.

**Separar o que o instrumento consegue medir do que ele não consegue**, e ancorar a conclusão só na
primeira parte, é o padrão que este artefato estabelece e que os posteriores herdam — em especial o
controle não modificado do [m46](/benchmarks/m46-highrecall-qps.md).

# Papel

Esta régua é o **gate** do milestone seguinte, e foi ela que previu que o ganho de quantização não
apareceria sem pressão de memória — previsão que o [m51](/benchmarks/m51-sbq-inline.md) confirmou.
