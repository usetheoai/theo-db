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

## 2026-07-30 (3) — mineração dos transcripts do projeto irmão

O owner apontou `projects/-home-paulo-Projetos-usetheo-theo-data-theo-db/memory` como fonte de aprendizados.

**Primeiro achado, e ele nega a premissa:** aquela memória é um **subconjunto estrito** da que já foi consolidada
— 64 de 65 arquivos **byte-idênticos**, e o `theo-cloud` ainda tem 2 arquivos a mais. Zero aprendizado novo ali.

**O que de fato não fora minerado:** os **562 MB de transcripts** do mesmo diretório (10 sessões, 4→27 de julho).
Extração de parágrafos com marcador de aprendizado: 497 distintos; 439 após descartar repetição de conceito já
coberto. Sete viraram conceito novo, dois atualizaram conceito existente:

| Novo | O que é |
|---|---|
| `nohup-em-ssh-nao-sobrevive` | `nohup &` dentro de `ssh` morre com o canal — exige `setsid` + verificação de PID. Custou duas corridas perdidas |
| `durable-rename-fsync-do-diretorio-pai` | 5 fsyncs em ordem estrita; o do diretório-pai é o load-bearing. E `durable_rename` NÃO faz PANIC |
| `dados-sinteticos-degenerados` | uniforme satura recall em 1.0 com `probes=1`; sem cluster despenca a 0.033. Nenhum dos dois mede o índice |
| `sbq-sem-vantagem-in-ram` | tese ≥2× falsificada: 0,31-0,77× do f32; a vantagem é memória, sob pressão de RAM |
| `pgduckdb-sobre-heap-e-mais-lento` | 0,52-0,78× do row-executor nativo, com plano DuckDB e resultado correto |
| `min-max-texto-e-colacao` | byte-min ≠ collation-min; determinismo não basta. Teto estrutural de ~35-39/43 no ClickBench |
| `juri-adversarial-precision-039` | 11 de 18 achados descartados pelo júri — ~1/3 de acionáveis é o esperado |

| Atualizado (regra § 4.3 — nunca bifurcar) | O que ganhou |
|---|---|
| `deriva-de-box-m168` | a instância do **M46: +122%** de deriva no controle de binário inalterado — 40× maior, e um ano antes |
| `superioridade-vetorial-vs-scann` | a causa-raiz é **problema de pesquisa** (grafo satura em 0,974 a 500k) e **3 levers já refutados** por medição |

**O mais desconfortável:** `nohup-em-ssh-nao-sobrevive` descreve um padrão que **usei várias vezes nesta própria
sessão** para lançar cargas na box de medição. Funcionou por sorte — a lição existia, registrada, e não estava
onde dispararia.
