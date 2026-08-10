# Skills

35 markdown-based skills that serve as entry-points for the Squad pipeline
and utilities. Claude Code discovers these automatically via the `SKILL.md`
frontmatter convention.

## Convention

Each skill lives in its own directory: `skills/{name}/SKILL.md`.

Required frontmatter fields:
- `name` — unique identifier (must match directory name)
- `description` — trigger phrase for Claude Code skill discovery
- `user-invocable` — `true` for slash-command skills

## Cycle Entry-Points

| Skill | Cycle | Purpose |
|---|---|---|
| `to-plan` | cycle-plan | Create implementation plan from a triaged item |
| `grill-me` | cycle-plan (Phase 0) | Interview for vague requirements |
| `edge-case-plan` | cycle-plan | Identify edge cases in a plan |
| `plan-confidence` | cycle-plan | Score plan structural quality |
| `plan-improve` | cycle-plan | Auto-improve plan score |
| `implement` | cycle-implement | TDD halt-loop execution |
| `code-quality` | cycle-code-quality | Dead code + fabricated symbol audit |
| `review` | cycle-review | Multi-agent parallel review |
| `release` | cycle-release | Semver tag + develop-to-main PR |
| `auto-plan` | cycle-auto-plan | End-to-end autonomous orchestrator |
| `backlog-item` | cycle-backlog | Register one item — a hypothesis, evidence not required |
| `discover-plan` | cycle-discover | Measurement plan: what is measured, and what would kill the hypothesis |
| `discover-edge-cases` | cycle-discover | What could make this measurement lie |
| `discover-plan-confidence` | cycle-discover | Score the measurement plan |
| `discover-execute` | cycle-discover | Run the measurement — may KILL the item |
| `discover-confidence` | cycle-discover | Score the opportunity |
| `discover-improve` | cycle-discover | Lift a low score — argument only, never the record |

## Utilities

| Skill | Purpose |
|---|---|
| `backlog-init` | Create BACKLOG.md once, inventorying repos from disk |
| `backlog-review` | Report what has rotted in the registry (read-only) |
| `ast-grep` | Structural search via tree-sitter |
| `cap-theorem-specialist` | Analyses CP/AP trade-offs in distributed architectures |
| `backpressure-specialist` | Diagnoses producer/consumer rate mismatch and flow control |
| `resilience-specialist` | Designs timeouts, retries, breakers, bulkheads and degradation |
| `deck` | Full presentation with diagrams |
| `marp-slide` | Marp slides only |
| `excalidraw` | Diagram JSON generation |
| `dogfood` | Honesty gate for v1.0 claims |
| `deps-audit` | Dependency CVE + version audit |
| `skill-creator` | Author / improve / eval any skill at `skills/{purpose}/` (official Anthropic skill-creator, standalone) |

## Adding a New Skill

1. Create `skills/{name}/SKILL.md` with required frontmatter
2. Add to the appropriate cycle contract in `rules/cycle-{name}.md`
3. Run `python3 scripts/check_xrefs.py` to validate cross-references
4. Run `python3 -m pytest tests/test_skill_frontmatter.py` to validate frontmatter
