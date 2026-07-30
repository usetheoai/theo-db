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
out=$(ssh -o BatchMode=yes host 'grep -oE "<padroes terminais>" log' 2>/dev/null)
rc=$?
if [ "$rc" -ne 0 ]; then
  unreach=$((unreach+1))
  # SILENCIO NAO E SUCESSO: avisa periodicamente que esta cego
  [ $((unreach % 15)) -eq 0 ] && echo "box inacessivel ha ~$((unreach*2)) min"
else
  [ "$unreach" -gt 0 ] && { echo "box voltou"; unreach=0; }
  [ -n "$out" ] && { echo "TERMINAL: $out"; break; }
fi
```

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
