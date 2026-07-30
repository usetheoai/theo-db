---
type: Technique
title: Num monitor, falha de transporte nunca pode parecer evento
description: Capturar stderr junto com stdout faz o erro de conexão virar 'evento terminal' — e o silêncio subsequente virar 'sucesso'.
tags: [monitoramento, operacao, shell]
timestamp: 2026-07-30T00:00:00Z
---

# Num monitor, falha de transporte nunca pode parecer evento

## O defeito que ensinou

```bash
out=$(ssh host 'grep ... log' 2>&1)     # <- 2>&1 captura o erro de SSH
if [ -n "$out" ]; then echo "TERMINAL: $out"; break; fi
```

Quando o SSH deu timeout, o texto `Connection timed out during banner exchange` entrou em `$out`, e o monitor
declarou desfecho terminal de uma carga que estava rodando normalmente.

## A forma correta

```bash
# `ssh` devolve o status do comando REMOTO — e `grep` sem casamento devolve 1. Por isso o teste
# NÃO pode ser `rc != 0`: no estado normal da vigília (log ainda sem marcador) isso seria 1 a
# cada iteração. Dois consertos, e os dois são necessários:
#   (a) neutralizar o rc do comando remoto com `|| true`;
#   (b) detectar falha de canal por 255, que é o código do PRÓPRIO ssh.
out=$(ssh -o BatchMode=yes host 'grep -oE "<padroes terminais>" log 2>/dev/null || true' 2>/dev/null)
rc=$?
if [ "$rc" -eq 255 ]; then
  unreach=$((unreach+1))
  # SILENCIO NAO E SUCESSO: avisa periodicamente que esta cego
  [ $((unreach % 15)) -eq 0 ] && echo "box inacessivel ha ~$((unreach*2)) min"
else
  [ "$unreach" -gt 0 ] && { echo "box voltou"; unreach=0; }
  [ -n "$out" ] && { echo "TERMINAL: $out"; break; }
fi
```

> **CORRIGIDO 2026-07-30 (round 3).** A versão anterior deste snippet testava `rc -ne 0` — e
> `man ssh` é explícito: *"ssh exits with the exit status of the remote command or with 255 if an error
> occurred."* Como `grep` sem casamento devolve 1, **todo poll saudável era contado como falha de
> transporte**: o monitor imprimiria "box inacessível" numa box perfeitamente acessível. A propriedade (1)
> que o texto abaixo declara não era obtida pelo código — a Technique prescrevia o defeito que ela existe
> para prevenir. **E eu rodei esse padrão nos monitores desta sessão.**

Três propriedades: (1) erro de transporte vai para `/dev/null` e é contado à parte; (2) o monitor **fala** quando
está cego, porque ausência de notícia não é notícia boa; (3) só conteúdo de log encerra a vigília.

## Cobertura dos desfechos

O filtro tem de casar **todos** os estados terminais, não só o feliz. Um monitor que procura apenas o marcador de
sucesso fica mudo durante crash, OOM e hang — e mudo é idêntico a "ainda rodando".

```
grep -oE "=== end.*|Traceback|out of memory|terminated by signal 9|server closed the connection"
```

## Relacionados

- [failure-mode/falso-verde-de-script](../failure-modes/falso-verde-de-script.md)
