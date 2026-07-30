---
type: Index
title: Técnicas
description: Índice dos conceitos do tipo `Technique` deste bundle.
tags: [okf, indice]
timestamp: 2026-07-30T00:00:00Z
---

# Técnicas

Métodos que **funcionaram** e por que funcionaram. Cada um nasceu de um modo de falha específico — o link
`Relacionados` no fim de cada arquivo aponta para o erro que o originou.

Se você só ler uma: [nenhuma-alegacao-sem-medicao](nenhuma-alegacao-sem-medicao.md).

| Conceito | O que é |
|---|---|
| [Um gap que é multiplicador ~constante ao longo do knob é custo por-candidato, não diferença algorítmica](a-forma-da-curva-diagnostica-a-causa.md) | A forma da curva separa custo fixo por candidato de diferença de algoritmo antes de qualquer profiler. |
| [Para medir um kernel, varie só o kernel](ablacao-mesmo-indice.md) | Comparar builds diferentes, boxes diferentes ou índices diferentes mede a soma das mudanças; a ablação sobre o MESMO artefato mede a mudança. |
| [Acervo local primeiro, web depois, memória do modelo por último](acervo-local-antes-da-web.md) | 25 PDFs e 33 repos versionados no acervo; citar arquivo:linha do disco é mais barato, offline e já passou pelo gate de licença. |
| [Prever o número com uma conta antes de medir](aritmetica-fechada-antes-do-experimento.md) | Uma previsão fechada transforma a medição em teste da hipótese, e um erro de ordem de grandeza denuncia o modelo mental antes de gastar horas de máquina. |
| [Meça um braço que NÃO mudou junto com o experimento](braco-de-controle-inalterado.md) | Se o binário inalterado lê +122% mais rápido entre dois runs, a box domina o sinal e nenhum veredito é possível. |
| [Um oráculo só é confiável se ele reprova um caso deliberadamente errado](controle-positivo.md) | Antes de confiar num verificador, prove que ele CONSEGUE reprovar — senão 'zero divergências' pode significar 'não olhou'. |
| [Intercalar os braços em vez de medi-los em blocos](desenho-ababab.md) | Comparar A e B em janelas separadas herda todo confundidor temporal; intercalar par a par o neutraliza — e a razão pareada mostra se o pareamento está funcionando. |
| [Todo gate declara o que conta como resultado — e recusa o resto](gate-de-nao-vacuidade.md) | Um gate sem definição explícita de desfecho observável não distingue 'passou' de 'não rodou'. |
| [Quando a pergunta é 'por que não roteia?', instrumente o caminho de decisão](instrumentar-em-vez-de-adivinhar.md) | Deduzir cobertura por leitura de SQL produz hipóteses; um trace das razões de declínio produz o mapa. |
| [Rodar a medição que separa 'nosso defeito' de 'propriedade do problema' antes de abrir o issue](medir-antes-de-filar.md) | Um issue com diagnóstico errado custa mais que a hora de medição que o teria evitado. |
| [Nenhuma alegação entra em documento antes da medição que a sustenta](nenhuma-alegacao-sem-medicao.md) | A regra-mãe do método deste projeto — e a metade que falha na prática é 'vale também para as alegações que me favorecem'. |
| [Todo log de medição carrega a identidade do binário e da máquina](proveniencia-em-todo-artefato.md) | Sem a identidade do binário e da máquina no cabeçalho, um artefato não é evidência — é um número solto. O exemplar do repo grava 2 dos 5 campos: dívida declarada, não regra cumprida. |
| [Num monitor, falha de transporte nunca pode parecer evento](separar-transporte-de-conteudo.md) | Capturar stderr junto com stdout faz o erro de conexão virar 'evento terminal' — e o silêncio subsequente virar 'sucesso'. |
