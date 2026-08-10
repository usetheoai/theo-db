/** Minimal db surface for the eval fixture — enough to make traces.ts readable and typed. */

export interface Span {
  id: string;
  traceId: string;
  parentSpanId: string | null;
  name: string;
  startedAt: string;
  endedAt: string | null;
}

export interface Trace {
  id: string;
  startedAt: string;
  rootSpanId: string;
  spans?: Span[];
}

export interface Db {
  query<T>(sql: string, params: unknown[]): Promise<T[]>;
  queryOne<T>(sql: string, params: unknown[]): Promise<T | null>;
}
