# Teste de mesa — a deriva da box invalida o A/B pareado do M168?

**Data:** 2026-07-29 · **Motivo:** o owner mandou parar o laço reativo e fazer o trabalho deliberado, depois de a
12ª rodada de revisão alegar que "ordem e efeito estão perfeitamente confundidos".

## Por que este documento existe

Doze rodadas de revisão do M168, e o padrão das últimas quatro foi sempre o mesmo: **eu corrigia uma alegação
aceitando o diagnóstico do revisor sem verificá-lo, e a correção introduzia um defeito novo.** A rodada 12 foi a
quarta iteração disso — o revisor disse "perfeitamente confundido", eu escrevi "perfeitamente confundido" no
verdict, e só então fiz a conta.

A conta diz outra coisa.

## A referência primária, e o que ela nomeia

`references/papers/rigorous-perf-eval-georges-2007.pdf` (Georges, Buytaert, Eeckhout, OOPSLA'07) — o paper que a
regra R3 deste projeto cita. § 2.1.2, *Experimental design*, *Other considerations*:

> "Other considerations concerning the experimental design include one hardware platform versus multiple hardware
> platforms; one heap size versus multiple heap sizes; a single VM implementation versus multiple VM
> implementations; and **back-to-back measurements ('aaabbb') versus interleaved measurements ('ababab')**."

Isso nomeia exatamente a estrutura do nosso problema, e a nomeia como **eixo de desenho experimental**, não como
detalhe. O ponto central do paper é que metodologias prevalentes "may yield misleading and even incorrect
conclusions" porque a análise de dados não é estatisticamente rigorosa (§ 1, Figura 1).

## O desenho que temos, nos dois níveis

| Nível | Estrutura | Protegido? |
|---|---|---|
| **Dentro** de uma coleta | `ababab` — o harness alterna eager/stream par a par, contrabalanceado | **SIM** — é a prescrição do paper |
| **Entre** coletas | `aaa…bbb` — A às 14:46, F às 22:13, uma por vez ao longo do dia | **NÃO** — é o anti-padrão que o paper nomeia |

Ou seja: a **magnitude de uma coleta** é defensável; a **comparação entre coletas** herda a estrutura errada.

## A medição que decide

Se a deriva da box vazasse para a razão, a razão marcharia com o tempo junto com os absolutos. Medido:

| | rho de Spearman vs ordem da coleta |
|---|---|
| `stream` **absoluto** | **+1,00** (monotonia perfeita: 132,4 → 134,0 → 142,6 → 145,2 → 148,7 → 155,6 ms) |
| `eager` absoluto | +0,94 |
| **efeito** (a razão pareada) | **+0,71** — crítico a n=6 para p<0,05 é **0,886** |

E a sequência do efeito **não é monotônica**: −0,6% · −0,7% · +4,7% · +4,5% · +2,9% · +6,7%. Três quebras
(B<A, D<C, **E<D**).

**Conclusão do teste de mesa: o pareamento está funcionando.** Os absolutos derivam de forma perfeita e o efeito
não deriva — que é precisamente o que se espera de um desenho `ababab` correto sob um confundidor temporal. Se o
confundidor estivesse vazando para a razão, o rho da razão também seria ~1,00.

### Onde o revisor estava certo, e onde exagerou

**Certo, e é uma falha minha real:** a tabela por-coleta que eu publiquei apresentava as seis como se fossem
intercambiáveis, **sem dizer que são uma série cronológica de um dia numa box compartilhada**. Omitir a ordem
quando as duas coletas dissidentes são exatamente as duas mais antigas é omitir a informação que o leitor precisa
para julgar. Isso vai para o documento.

**Exagerado:** "ordem e efeito perfeitamente confundidos" e "cada coleta acrescentada subiu a média". O primeiro é
negado pelo rho de +0,71 com três quebras. O segundo é aritmeticamente verdadeiro mas não implica deriva — a média
sobe sempre que o valor acrescentado excede a média corrente, e E (+2,9%) é **menor** que D (+4,5%).

**E eu errei ao aceitar sem verificar.** Escrevi "PERFEITAMENTE CONFUNDIDOS" em negrito no verdict antes de rodar
o rho da razão. É a quarta rodada consecutiva em que a correção introduz o defeito.

## O sinal que sobra, e é o mais interessante da série

Os dois braços **não** degradaram igualmente de A para F:

| | A | F | Δ |
|---|---|---|---|
| `eager` | 133,8 ms | 151,9 ms | **+13,5%** |
| `stream` | 132,4 ms | 155,6 ms | **+17,5%** |

O braço streaming degradou **1,30× mais**. Isso não é confundidor — é **hipótese mecânica testável**, e ela casa
com a arquitetura: o caminho streaming faz **100 travessias de plano do DataFusion** contra 1 do eager. Mais
travessias = mais oportunidades de ser desescalonado sob contenção.

Se verdadeira, a leitura correta das projeções estreitas **não** é "custo de 2%" nem "sem efeito". É:

> **numa box ociosa o custo é ~0; sob contenção ele aparece, e cresce com a contenção.**

Isso explica os dados melhor que qualquer das minhas duas versões anteriores: A e B (box mais leve) dão ganho
marginal; C, D, F (box carregada) dão custo; E é a exceção que impede a leitura simplista de deriva monotônica.

E o q23 mostra o mesmo padrão de forma consistente: eager +11,0%, stream +13,5% de A para F — o streaming também
degradou mais lá, mas o ganho de 18% é grande o bastante para sobreviver (razão de 0,805 a 0,823, estável).

## O controle decisivo, e ele está rodando

O paper prescreve converter `aaabbb` em `ababab`. Aplicado ao nível de coleta:

**reconstruir o binário da coleta A e rodá-lo INTERCALADO com o de F, numa única janela.**

- **A-agora mostra custo** → a diferença entre coletas é da box, não do código. O efeito das estreitas é
  "aparece sob contenção", e a tabela por-coleta não sustenta inferência entre coletas.
- **A-agora ainda mostra ganho** → há diferença real de código entre A e F, e o pooling entre coletas é inválido
  por outra razão (binários não equivalentes no caminho quente).

Qualquer um dos dois resultados é publicável e fecha a pergunta. O build de A está em curso
(`/root/theo-db-A`, `git archive 6133b9f`).

**Nota honesta sobre a expectativa:** o único delta de código no caminho quente entre A e F são duas leituras de
`i32` por chunk-group (`ClientConnectionLost`, `TransactionTimeoutPending`), que a ~100 chunk-groups não produzem
5% de uma consulta de 140 ms. Então eu **espero** o primeiro resultado. Registro a expectativa antes de medir,
para que o resultado não seja lido como confirmação retroativa.

## O que muda no verdict

1. **Publicar a cronologia** — a tabela por-coleta ganha coluna de horário. Isso é dívida minha, não do revisor.
2. **Corrigir "perfeitamente confundidos"** para o que a medição sustenta: os absolutos derivam, o efeito não;
   o pareamento protege; a comparação **entre** coletas é que herda a estrutura `aaabbb`.
3. **Reformular a leitura das estreitas** de "custo de ~2%" para a hipótese de sensibilidade a contenção,
   marcada como `UNBENCHMARKED` até o controle voltar.
4. **Manter o teste t nível-coleta fora do documento** — não porque o confundidor é provado, mas porque a
   permutabilidade que ele assume não é verificável com n=6 numa série temporal.
5. **A magnitude do q23 (13,6%) não é afetada** — ela é intra-coleta, e o q23 é 12/12 em cada uma das seis.

## Lição de método para o resto da série

O erro que se repetiu quatro vezes não foi estatístico. Foi **aceitar um diagnóstico bem-argumentado sem fazer a
conta que o testaria**. Os revisores desta série são bons justamente porque constroem controles em vez de
raciocinar — e eu vinha respondendo com prosa em vez de contas.

Regra que adoto daqui em diante nesta série: **nenhuma alegação de revisor entra no documento antes de eu
reproduzir a medição que a sustenta.** Vale igualmente para as que me contradizem e para as que me favorecem.
