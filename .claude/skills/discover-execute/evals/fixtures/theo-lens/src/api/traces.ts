/**
 * Trace listing endpoint — eval fixture.
 *
 * This file carries a REAL N+1: `listTraces` issues one query per span, in a loop. It is
 * here so `/discover --mode review` has something to actually measure instead of a
 * hypothetical. A measurement eval whose target does not exist tests the agent's
 * imagination, not the skill.
 *
 * Counterpart: `listTracesBatched` below does the same job in one query. An eval that
 * points at THAT function must find the hypothesis refuted — which is how the kill path
 * gets exercised against real code rather than a stub.
 */

import type { Db, Span, Trace } from "../db";

export interface ListTracesOptions {
  windowDays: number;
  tenantId: string;
}

/**
 * Lists traces with their spans.
 *
 * Known shape: one query for the trace list, then one per trace for its span ids, then
 * one per span. A single trace with 200 spans costs 202 queries (1 + 1 + 200).
 *
 * The arithmetic is spelled out because an eval run caught this docstring claiming 201 —
 * it omitted the per-trace query below. A fixture whose comment misstates its own defect
 * teaches the wrong number to whoever measures it.
 */
export async function listTraces(db: Db, opts: ListTracesOptions): Promise<Trace[]> {
  const traces = await db.query<Trace>(
    "SELECT id, started_at, root_span_id FROM traces WHERE tenant_id = $1 AND started_at > now() - ($2 || ' days')::interval",
    [opts.tenantId, opts.windowDays],
  );

  for (const trace of traces) {
    const spanIds = await db.query<{ id: string }>(
      "SELECT id FROM spans WHERE trace_id = $1",
      [trace.id],
    );

    trace.spans = [];
    for (const { id } of spanIds) {
      // One round-trip per span. This is the defect the fixture exists to expose.
      const span = await db.queryOne<Span>(
        "SELECT id, parent_span_id, name, started_at, ended_at FROM spans WHERE id = $1",
        [id],
      );
      if (span) {
        trace.spans.push(span);
      }
    }
  }

  return traces;
}

/**
 * The batched form: two queries total, regardless of span count.
 *
 * Not wired into the router — it exists so a measurement can compare the two shapes, and
 * so an eval pointed here finds a hypothesis about N+1 genuinely refuted.
 */
export async function listTracesBatched(db: Db, opts: ListTracesOptions): Promise<Trace[]> {
  const traces = await db.query<Trace>(
    "SELECT id, started_at, root_span_id FROM traces WHERE tenant_id = $1 AND started_at > now() - ($2 || ' days')::interval",
    [opts.tenantId, opts.windowDays],
  );
  if (traces.length === 0) return traces;

  const spans = await db.query<Span>(
    "SELECT id, trace_id, parent_span_id, name, started_at, ended_at FROM spans WHERE trace_id = ANY($1)",
    [traces.map((t) => t.id)],
  );

  const byTrace = new Map<string, Span[]>();
  for (const span of spans) {
    const bucket = byTrace.get(span.traceId) ?? [];
    bucket.push(span);
    byTrace.set(span.traceId, bucket);
  }

  for (const trace of traces) {
    trace.spans = byTrace.get(trace.id) ?? [];
  }

  return traces;
}
