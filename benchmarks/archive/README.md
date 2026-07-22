# benchmarks/archive

Harness de reprodução de milestones já lançados que **não são referenciados** por
nenhum lugar do repo (0 refs em docs/benchmarks, ADR, CI, testes ou `.claude/`) e não
são importados por nenhum outro `.py`. Movidos para cá (não apagados) para desafogar o
topo de `benchmarks/` mantendo a evidência reproduzível — conforme
`.claude/rules/audit-trail-rotation.md` (**arquivar, nunca deletar** evidência).

- **Nada foi perdido:** cada script continua buscável e versionado.
- **Zero import quebrado no CI:** nenhum era coletado pelo pytest (não são `test_*.py`)
  nem importado por runner vivo.
- **Contexto:** os milestones m84–m87 têm doc de evidência em `docs/benchmarks/`
  (`m84-recall-confirmation`, `m85-sq8-refine`, `m86-soar-spill`, `m87-filtered-ann`),
  mas esses docs nunca linkaram o `.py` de repro — daí o status de órfão. `e1_sweep_only`
  e `run_m60_efc` são variantes intermediárias dos irmãos citados (`e1_rabitq_bench`,
  `run_m60_recall`). Se algum voltar a ser citado, mova-o de volta ao topo.
