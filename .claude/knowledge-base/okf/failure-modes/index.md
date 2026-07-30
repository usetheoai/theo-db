---
type: Index
title: Modos de falha
description: Índice dos conceitos do tipo `Failure Mode` deste bundle.
tags: [okf, indice]
timestamp: 2026-07-30T00:00:00Z
---

# Modos de falha

Cada entrada é uma classe de erro que este projeto **cometeu** e pagou. Elas estão aqui não como confissão, mas
porque a assinatura de cada uma é reconhecível **antes** do custo — e reconhecer é o único jeito de não repetir.

Leia antes de montar qualquer medição. Metade destas entradas são medições que pareciam válidas e não eram.

| Conceito | O que é |
|---|---|
| [O A/B do benchmark prova o espaço de DADOS; o review prova o espaço de TIPOS](ab-prova-o-espaco-de-dados-nao-o-de-tipos.md) | Cinco milestones seguidos tiveram HIGH/BLOCKER invisíveis ao diverged=0, porque os dados do ClickBench não exercitam o espaço de tipos. |
| [Uma allowlist por regex sobre uma LINGUAGEM é bypassável — a mesma defesa caiu duas vezes seguidas](allowlist-por-regex-sobre-linguagem.md) | O allowlist de relações do NL→SQL foi furado por vírgula-join e depois por identificador entre aspas; regex não conhece a gramática que tenta restringir. |
| [Um assert que é uma IDENTIDADE passa sempre e não prova nada — e o gate parece verde](assert-que-e-uma-identidade.md) | O parity-gate assertava memória com uma expressão algebricamente equivalente aos dois lados, e o gate de recall não isolava o quantizador porque o carrier f32 + rerank dominavam. |
| [109 artefatos de benchmark e nenhum inicializa a aplicação](benchmark-nao-prova-que-o-produto-funciona.md) | Uma suíte de benchmark mede algoritmos; ela é estruturalmente incapaz de descobrir que o produto não inicializa para um consumidor real. |
| [Alegar cobertura que a ferramenta não produziu](cobertura-alegada-sem-execucao.md) | Um auditor indisponível, um detector que não rodou ou um teste que não existe viram 'nada encontrado' em vez de 'não verificado'. |
| [drop_caches uma vez por sweep mede a PRIMEIRA query fria e 99 quentes — o resultado é um limite inferior](cold-medido-uma-vez-por-sweep.md) | O +21% de cold-QPS do M88 é consistente com a tese e não é uma medição limpa de crossover; o artefato diz isso, e quem cita só o número perde a ressalva. |
| [Uma configuração escolhida pelo operador torna o sistema inmedível](config-do-operador-que-inviabiliza-a-medicao.md) | Um knob mexido com boa intenção (acelerar a carga) muda o regime a ponto de o experimento não poder rodar — e o sintoma parece bug do produto. |
| [Comparar dois retrievers cujos candidate-sets têm semânticas diferentes mede o filtro, não o ranker](conflacao-ranker-com-candidate-set.md) | Três instâncias pagas: corpus menor que o top-k, filtro booleano que dropa 93% dos relevantes, e parser AND vs OR. |
| [Medir com carga concorrente e atribuir o resultado ao código](contaminacao-por-concorrencia.md) | Qualquer processo competindo pela box durante uma medição — inclusive um do próprio operador — desloca o número, e o deslocamento vira propriedade alegada do código. |
| [Corrigir o conceito-fonte e deixar os que o citam com o valor antigo](correcao-nao-propagada-pelo-grafo.md) | Um número corrigido em um arquivo sobrevive nos siblings que o citam — inclusive no índice — e o registro afirma que a correção foi concluída. |
| [Congelar uma crença intermediária como se fosse conclusão](crenca-intermediaria-congelada.md) | Minerar transcripts (deliberação em andamento) sem cruzar com memória consolidada e artefato produz conceitos que parecem verificados e registram o que se acreditava no meio do caminho. |
| [Dados sintéticos degenerados produzem recall absurdo — para cima ou para baixo, e nenhum dos dois mede o índice](dados-sinteticos-degenerados.md) | Vetores uniformes de alta dimensão saturam recall em 1.0 mesmo com probes=1 — qualquer índice é indistinguível ali. O valor absoluto não mede o algoritmo; só o diferencial entre braços sobre o MESMO dataset mede. |
| [Aceitar um diagnóstico bem-argumentado sem refazer a conta](diagnostico-aceito-sem-reproduzir.md) | Um revisor (ou eu mesmo) apresenta uma explicação coerente; ela entra no documento sem que a medição que a sustentaria seja reproduzida. As que mais escapam são as que favorecem quem escreve. |
| [Reconstruir rastro de ciclo depois do fato](documentacao-retroativa-como-gate.md) | Um milestone que shipou sem log de implementação nem review tenta 'regularizar' escrevendo os documentos a partir dos commits — alto risco de fabricação, zero valor de gate. |
| [Duas sessões trabalhando no mesmo working tree](duas-sessoes-num-checkout.md) | Dois agentes no mesmo checkout trocam de branch por baixo um do outro; o segundo encontra a árvore sem seus arquivos e pode concluir que perdeu trabalho. |
| [Absorver um achado no milestone só porque é do mesmo tema](escopo-que-cresce-por-afinidade-tematica.md) | Um defeito real, descoberto durante o milestone, é puxado para dentro dele por parentesco conceitual — e o milestone passa a ter dois Goals. |
| [Aplicar o teste errado e publicar a significância dele](estatistica-que-nao-sustenta-a-alegacao.md) | Empate contado como derrota, família de multiplicidade errada, clustering ignorado, e a magnitude tirada da coleta mais lisonjeira. |
| [EXPLAIN ANALYZE taxa por linha e fabrica speedup entre braços de tamanhos diferentes](explain-analyze-e-instrumento-assimetrico.md) | A instrumentação por-tupla infla o braço que produz mais linhas; medir com \timing na query nua deu 1,60× onde o EXPLAIN ANALYZE lia 1,64×. |
| [O caminho de validação não previsto roda SEM a restrição](fail-open-por-omissao.md) | Um filtro que valida a forma esperada e ignora a inesperada não falha — ele executa sem o filtro, que é o pior desfecho possível. |
| [O script relata sucesso que não houve](falso-verde-de-script.md) | Captura de código de saída do comando errado, gate que casa string inexistente, ou `rc=0` impresso depois de um `echo` — o log mente e ninguém confere. |
| [Um gate que não reconhece a entrada e pula sem avisar](gate-desligado-em-silencio.md) | O gate procura um literal exato; o artefato usa outro; ele não falha — ele SKIPa, e a ausência de reclamação é lida como aprovação. |
| [Um guard colocado ANTES do passo que completa o estado transforma dado presente em zero linhas, em silêncio](guard-antes-de-materializar-o-pendente.md) | scan_ivf_structured retornava cedo em centroides vazios antes de dobrar a região pendente — um índice vazio com INSERTs depois devolvia zero linhas sem erro. |
| [O instrumento não observa a coisa que se quer medir](instrumento-cego-a-arquitetura.md) | Escolher um contador que a arquitetura do sistema torna estruturalmente incapaz de ver o fenômeno — e ler o zero dele como ausência do fenômeno. |
| [Medição degenerada aceita como dado](medicao-vacuosa-aceita.md) | Um resultado que só poderia sair de um setup quebrado (zero linhas, tempo impossível, zero divergências num oráculo que nunca rodou) é registrado como se fosse observação. |
| [Um resumo funde dois números em um, e a cadeia de citação propaga o erro](numero-comprimido-na-cadeia-de-citacao.md) | Artefato mede duas grandezas; o ADR as comprime numa; o conceito cita o ADR e herda o erro — e o ADR cita o artefato que o contradiz. |
| [O oráculo de correção é O(N) e morre antes do sistema](oraculo-de-correcao-que-nao-escala.md) | O guard que prova correção puxa o resultado inteiro para o cliente; na escala alvo ele estoura antes de qualquer conclusão sobre o produto. |
| [Um A/B que compara só agregados é cego a uma chave de GROUP BY errada](oraculo-que-nao-compara-a-chave.md) | count e sum sobrevivem ao colapso da chave; o oráculo tem de comparar a COLUNA-CHAVE por symmetric-EXCEPT, senão um erro de epoch passa como diverged=0. |
| [O teste passa — e passaria também sem o fix](teste-que-passa-pela-razao-errada.md) | Um teste que não falha no código ANTIGO não prova nada; validação real pegou três defeitos assim nos meus próprios testes. |
