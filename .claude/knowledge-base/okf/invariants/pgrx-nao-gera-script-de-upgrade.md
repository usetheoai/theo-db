---
type: Invariant
title: pgrx não gera o script de upgrade — e uma pg_extern nova não alcança catálogo existente
description: Adicionar uma função depois que default_version foi congelada faz o símbolo existir no .so e não no catálogo; o erro só aparece em instalação pré-existente.
tags: [pgrx, postgres, extensao, upgrade]
timestamp: 2026-07-30T00:00:00Z
---

# pgrx não gera o script de upgrade — e uma `pg_extern` nova não alcança catálogo existente

## O invariante

`cargo pgrx install` gera o SQL da versão **corrente**, mas **não** gera o script de migração
`extensão--A--B.sql`. Se uma `#[pg_extern]` é adicionada depois de `default_version` ter sido congelada, ela:

- existe no `.so`;
- existe no SQL da versão nova;
- **não chega** a um catálogo que já tem a extensão instalada.

O sintoma é `function ... does not exist` **só** em ambiente pré-existente — instalação nova funciona, o que
esconde o defeito.

## Casos

`theodb_columnar_chunks_scanned` (M150) e as funções do #219 caíram exatamente aqui.

## Armadilhas irmãs (M137)

- **Regex de ancoragem** no script de upgrade: padrão frouxo casa mais do que devia.
- **SQLSTATE presumido**: assumir o código de erro sem verificar leva a `EXCEPTION WHEN` que não pega.
- **Corrupção silenciosa de shell type**: um tipo criado como shell e nunca completado passa despercebido.

## Regra derivada

Toda `pg_extern` nova exige entrada na cadeia de upgrade **no mesmo commit**, e um teste que instale a versão
anterior e migre.
