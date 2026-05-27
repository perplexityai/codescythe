export interface RunOptions {
  cwd?: string;
  config?: string;
  fix?: boolean;
  verbose?: boolean;
  force?: boolean;
  explainExport?: string;
}

export interface IgnoredUnresolvedImportSample {
  specifier: string;
  importer: string;
}

export interface IgnoredUnresolvedImportsByPattern {
  pattern: string;
  count: number;
  samples: IgnoredUnresolvedImportSample[];
}

export interface SourceAliasIgnoreWarning {
  pattern: string;
  alias: string;
  source: string;
  fixBlocking: boolean;
  message: string;
}

export interface ExportExplanation {
  exportingFile: string;
  symbol: string;
  fileReachable: boolean;
  importersConsidered: { importer: string; specifier: string; reason: string }[];
  importersSkipped: { importer: string; specifier: string; reason: string }[];
  ignoredUnresolvedImportsThatMightHavePointedAtThisFile: IgnoredUnresolvedImportSample[];
}

export interface ExplainExportResult {
  exportingFile: string;
  symbol: string;
  status: 'alive' | 'dead' | 'fileUnused' | 'fileNotFound' | 'symbolNotExported';
  reason: string;
  explanation?: ExportExplanation;
}

export interface AnalysisSummary {
  version: string;
  configPath?: string | null;
  projectCount: number;
  entryCount: number;
  ignoredUnresolvedCount: number;
  ignoredUnresolvedPatterns: string[];
  packageImportKeys: string[];
  configuredAliasKeys: string[];
}

export interface ConfigDoctorResult {
  warnings: { code: string; message: string }[];
  summary: AnalysisSummary;
  unresolvedImports?: UnresolvedImportExplanation[];
}

export interface UnresolvedImportExplanation {
  importer: string;
  specifier: string;
  resolverError: string;
  matchedAliases: UnresolvedImportMatchedAlias[];
}

export interface UnresolvedImportMatchedAlias {
  source: string;
  key: string;
  target: string;
  expandedTarget: string;
  candidateFiles: UnresolvedImportCandidateFile[];
}

export interface UnresolvedImportCandidateFile {
  path: string;
  exists: boolean;
  inProject: boolean;
}

export interface Analysis {
  issues: {
    files: Record<string, { path: string }>;
    exports: Record<string, Record<string, { symbol: string; kind: 'value' | 'type'; line: number; col: number; explanation?: ExportExplanation }>>;
    unresolved?: Record<string, string[]>;
  };
  counters: {
    files: number;
    exports: number;
    unresolved: number;
    processed: number;
    total: number;
    ignoredUnresolved?: number;
  };
  summary?: AnalysisSummary;
  ignoredUnresolvedImportsByPattern?: Record<string, IgnoredUnresolvedImportsByPattern>;
  sourceAliasIgnoreWarnings?: SourceAliasIgnoreWarning[];
  explainExport?: ExplainExportResult;
}

export interface FixResult {
  changedFiles: string[];
  removedFiles: string[];
  skippedExportFiles?: string[];
  removedExports: number;
  analysis: Analysis;
}

export interface NativeBinding {
  analyze(options?: RunOptions): string;
  doctor(options?: RunOptions): string;
  fix(options?: RunOptions): string;
}

export function analyze(options?: RunOptions): Analysis;
export function doctor(options?: RunOptions): ConfigDoctorResult;
export function fix(options?: RunOptions): FixResult;
export const native: NativeBinding;

declare const codescythe: {
  analyze: typeof analyze;
  doctor: typeof doctor;
  fix: typeof fix;
  native: NativeBinding;
};

export default codescythe;
