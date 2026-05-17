export interface RunOptions {
  cwd?: string;
  config?: string;
  fix?: boolean;
}

export interface Analysis {
  issues: {
    files: Record<string, { path: string }>;
    exports: Record<string, Record<string, { symbol: string; kind: 'value' | 'type'; line: number; col: number }>>;
    unresolved?: Record<string, string[]>;
  };
  counters: {
    files: number;
    exports: number;
    unresolved: number;
    processed: number;
    total: number;
  };
}

export interface FixResult {
  changedFiles: string[];
  removedExports: number;
  analysis: Analysis;
}

export function analyze(options?: RunOptions): Analysis;
export function fix(options?: RunOptions): FixResult;
