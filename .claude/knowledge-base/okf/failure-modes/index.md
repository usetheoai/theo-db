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
| [109 artefatos de benchmark e nenhum inicializa a aplicação](benchmark-nao-prova-que-o-produto-funciona.md) | Uma suíte de benchmark mede algoritmos; ela é estruturalmente incapaz de descobrir que o produto não inicializa para um consumidor real. |
| [Alegar cobertura que a ferramenta não produziu](cobertura-alegada-sem-execucao.md) | Um auditor indisponível, um detector que não rodou ou um teste que não existe viram 'nada encontrado' em vez de 'não verificado'. |
| [Uma configuração escolhida pelo operador torna o sistema inmedível](config-do-operador-que-inviabiliza-a-medicao.md) | Um knob mexido com boa intenção (acelerar a carga) muda o regime a ponto de o experimento não poder rodar — e o sintoma parece bug do produto. |
| [Medir com carga concorrente e atribuir o resultado ao código](contaminacao-por-concorrencia.md) | Qualquer processo competindo pela box durante uma medição — inclusive um do próprio operador — desloca o número, e o deslocamento vira propriedade alegada do código. |
| [Congelar uma crença intermediária como se fosse conclusão](crenca-intermediaria-congelada.md) | Minerar transcripts (deliberação em andamento) sem cruzar com memória consolidada e artefato produz conceitos que parecem verificados e registram o que se acreditava no meio do caminho. |
| [Dados sintéticos degenerados produzem recall absurdo — para cima ou para baixo](dados-sinteticos-degenerados.md) | Vetores uniformes de alta dimensão saturam recall em 1.0 mesmo com probes=1; sem clusters, o recall despenca a 0.033. Nenhum dos dois mede o algoritmo. |
| [Aceitar um diagnóstico bem-argumentado sem refazer a conta](diagnostico-aceito-sem-reproduzir.md) | Um revisor (ou eu mesmo) apresenta uma explicação coerente; ela entra no documento sem que a medição que a sustentaria seja reproduzida. As que mais escapam são as que favorecem quem escreve. |
| [Reconstruir rastro de ciclo depois do fato](documentacao-retroativa-como-gate.md) | Um milestone que shipou sem log de implementação nem review tenta 'regularizar' escrevendo os documentos a partir dos commits — alto risco de fabricação, zero valor de gate. |
| [Duas sessões trabalhando no mesmo working tree](duas-sessoes-num-checkout.md) | Dois agentes no mesmo checkout trocam de branch por baixo um do outro; o segundo encontra a árvore sem seus arquivos e pode concluir que perdeu trabalho. |
| [Absorver um achado no milestone só porque é do mesmo tema](escopo-que-cresce-por-afinidade-tematica.md) | Um defeito real, descoberto durante o milestone, é puxado para dentro dele por parentesco conceitual — e o milestone passa a ter dois Goals. |
| [Aplicar o teste errado e publicar a significância dele](estatistica-que-nao-sustenta-a-alegacao.md) | Empate contado como derrota, família de multiplicidade errada, clustering ignorado, e a magnitude tirada da coleta mais lisonjeira. |
| [O caminho de validação não previsto roda SEM a restrição](fail-open-por-omissao.md) | Um filtro que valida a forma esperada e ignora a inesperada não falha — ele executa sem o filtro, que é o pior desfecho possível. |
| [O script relata sucesso que não houve](falso-verde-de-script.md) | Captura de código de saída do comando errado, gate que casa string inexistente, ou `rc=0` impresso depois de um `echo` — o log mente e ninguém confere. |
| [Um gate que não reconhece a entrada e pula sem avisar](gate-desligado-em-silencio.md) | O gate procura um literal exato; o artefato usa outro; ele não falha — ele SKIPa, e a ausência de reclamação é lida como aprovação. |
| [O instrumento não observa a coisa que se quer medir](instrumento-cego-a-arquitetura.md) | Escolher um contador que a arquitetura do sistema torna estruturalmente incapaz de ver o fenômeno — e ler o zero dele como ausência do fenômeno. |
| [Medição degenerada aceita como dado](medicao-vacuosa-aceita.md) | Um resultado que só poderia sair de um setup quebrado (zero linhas, tempo impossível, zero divergências num oráculo que nunca rodou) é registrado como se fosse observação. |
| [O oráculo de correção é O(N) e morre antes do sistema](oraculo-de-correcao-que-nao-escala.md) | O guard que prova correção puxa o resultado inteiro para o cliente; na escala alvo ele estoura antes de qualquer conclusão sobre o produto. |
| [O teste passa — e passaria também sem o fix](teste-que-passa-pela-razao-errada.md) | Um teste que não falha no código ANTIGO não prova nada; validação real pegou três defeitos assim nos meus próprios testes. |
