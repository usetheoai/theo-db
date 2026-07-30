---
type: Log
title: Histórico do bundle
description: Registro cronológico de quando cada bloco de conhecimento entrou e o que o motivou.
tags: [okf, historico]
timestamp: 2026-07-30T00:00:00Z
---

# Log

## 2026-07-30 — criação do bundle

Motivador imediato: uma sessão de trabalho no M169 em que **seis** alegações minhas foram derrubadas por medição
(#219, #220 duas vezes, EC-2, "q20 nunca observado", linha fabricada do EC-1, custo do ADR-5), mais **quatro**
defeitos de instrumentação numa única medição de memória. Nenhum deles era novo em espécie — todos tinham
precedente registrado em memória do projeto, e nenhum estava num lugar que disparasse no momento certo.

Fontes consolidadas: 67 arquivos de memória do projeto (M46→M169), o desk-check do M168, as notas de
implementação do M169, e as mensagens de commit da série.

Escopo deliberadamente **não** incluído: planos, reviews, ADRs e audits históricos. Eles continuam em
`knowledge-base/`, no formato do ciclo. Este bundle é sobre **método e invariantes**, não sobre o rastro de
execução.

## 2026-07-30 — o bundle ganha contrato, validador e gates

Criar o bundle não bastava: um bundle que ninguém lê é pior que nenhum, porque produz a sensação de cobertura
sem a cobertura. Três mecanismos foram acrescentados no mesmo dia:

| Peça | O que faz | Grau |
|---|---|---|
| `rules/okf-knowledge-base.md` | o contrato — quando ler, quando escrever, o que é máquina e o que não é | contrato |
| `scripts/check_okf.py` | valida 4 invariantes estruturais (C1 `type`, C2 links, C3 índices, C4 raiz) | **determinístico** |
| `hooks/stop-validation.sh` gate 5 | BLOQUEIA em bundle inválido, e em número publicado sem `Measurement` | **hard gate** |
| `hooks/userpromptsubmit-inject.sh` | injeta o ponteiro a cada turno, ao lado da parsimony ladder | injeção |

O validador tem **controle positivo**: um bundle deliberadamente quebrado tem de produzir exit 1, e produz
(C1+C2+C3 detectados). Sem isso ele seria o `cobertura-alegada-sem-execucao` que este mesmo bundle documenta.

Durante a construção dos testes, **dois** dos meus próprios modos de falha catalogados reapareceram — e é o dado
mais interessante do dia: capturei `$?` de um `tail` num pipeline (`falso-verde-de-script`) e testei o gate de
benchmark com um arquivo não-rastreado, que `ALL_FILES` estruturalmente não vê (`instrumento-cego-a-arquitetura`).
O catálogo pegou os dois porque eu tinha acabado de escrevê-los.

## 2026-07-30 (2) — auditoria de cobertura: 7 lacunas reais encontradas e fechadas

O owner perguntou "todos os aprendizados estão no OKF?". Eu tinha **afirmado** consolidar 67 arquivos de memória
sem nunca verificar entrada por entrada — o `cobertura-alegada-sem-execucao` aplicado ao próprio bundle.

Medido: 10 memórias sem rastro algum. Lidas uma a uma e classificadas:

| Veredito | Quantas | Ação |
|---|---|---|
| lacuna real | **7** | conceito escrito |
| corretamente fora (§ 4.2 — rastro de execução, ou credencial) | 2 | nenhuma |
| falso negativo da minha própria busca | 1 | nenhuma (`m140-4` está coberto sob `Spi`) |

Conceitos acrescentados: `benchmark-nao-prova-que-o-produto-funciona`, `teste-que-passa-pela-razao-errada`,
`fail-open-por-omissao`, `bgworker-transaction-segura-snapshot`, `worker-nao-ve-set-de-sessao`,
`datafusion-sum-int64-faz-wrapping`, `customscan-scanrelid-zero-e-aggref-pullup`.

**Ressalva que fica registrada porque é o dado mais honesto daqui:** a heurística que usei erra nos DOIS sentidos.
Deu falso negativo em `m140-4` (busquei termos do slug; a lição vive sob `Spi::get_one`), e "com rastro" para as
outras 56 significa apenas que **uma palavra apareceu em algum lugar** — não que a lição virou conceito. Logo
**56/66 é teto, não medida**, e a cobertura real das 56 continua não auditada. Além disso, os 110 blueprints e as
mensagens de commit da série **nunca foram varridos** — é superfície maior que a das memórias.
