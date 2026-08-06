# theo-lens — eval fixture

Not the real repo. A minimal stand-in so measurement evals have code to actually open,
count and cite.

`src/api/traces.ts` carries a **real N+1**: `listTraces` issues one query per span in a
loop. `listTracesBatched` in the same file does the job in two queries regardless of span
count — it is there so a kill-path eval can point at a function where the N+1 hypothesis
is genuinely refuted, against real code rather than a stub.

A `pnpm-lock.yaml` is present because the six data-plane repos use pnpm, and a specialist
that checks the lockfile before choosing a command should find the right answer here.
