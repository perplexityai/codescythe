#!/usr/bin/env -S node --experimental-transform-types
'use strict';

type CliOptions = {
  config?: string;
  cwd?: string;
  explainExport?: string;
  force?: boolean;
  verbose?: boolean;
};

type AnalysisResult = {
  counters: {
    exports: number;
    files: number;
    unresolved: number;
  };
  issues: {
    exports: Record<string, Record<string, {col: number; line: number; symbol: string}>>;
    files: Record<string, unknown>;
  };
  explainExport?: unknown;
};

type FixResult = {
  changedFiles: string[];
  removedFiles: string[];
  skippedExportFiles?: string[];
  removedExports: number;
  analysis: AnalysisResult;
};

type DoctorResult = {
  warnings: {code: string; message: string}[];
};

const { analyze, doctor, fix } = require('codescythe') as {
  analyze(options: CliOptions): AnalysisResult;
  doctor(options: CliOptions): DoctorResult;
  fix(options: CliOptions): FixResult;
};

const args = process.argv.slice(2);
const command = args[0] === 'doctor' ? args.shift() : undefined;
const options: CliOptions = {};
let json = false;
let shouldFix = false;

for (let i = 0; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--json') {
    json = true;
  } else if (arg === '--fix') {
    shouldFix = true;
  } else if (arg === '--force') {
    options.force = true;
  } else if (arg === '--verbose') {
    options.verbose = true;
  } else if (arg === '--explain-export') {
    options.explainExport = args[++i];
  } else if (arg === '--config' || arg === '-c') {
    options.config = args[++i];
  } else if (arg === '--directory' || arg === '-C') {
    options.cwd = args[++i];
  } else {
    console.error(`Unknown argument: ${arg}`);
    process.exit(2);
  }
}

const result = command === 'doctor' ? doctor(options) : shouldFix ? fix(options) : analyze(options);
if (json) {
  console.log(JSON.stringify(result));
} else if (command === 'doctor') {
  const doctorResult = result as DoctorResult;
  if (doctorResult.warnings.length === 0) {
    console.log('No risky Codescythe config found');
  } else {
    for (const warning of doctorResult.warnings) {
      console.log(`${warning.code}: ${warning.message}`);
    }
  }
} else if (options.explainExport) {
  console.log(JSON.stringify((result as AnalysisResult).explainExport));
} else if (shouldFix) {
  const fixResult = result as FixResult;
  console.log(
    `Removed ${fixResult.removedExports} unused exports from ${fixResult.changedFiles.length} files and ${fixResult.removedFiles.length} unused files`,
  );
  if (fixResult.skippedExportFiles?.length) {
    console.log(`Skipped export edits in ${fixResult.skippedExportFiles.length} files because ignored unresolved imports create alias uncertainty`);
  }
} else {
  const analysisResult = result as AnalysisResult;
  if (analysisResult.counters.files === 0 && analysisResult.counters.exports === 0 && analysisResult.counters.unresolved === 0) {
    console.log('No dead TypeScript code found');
  } else {
    for (const filePath of Object.keys(analysisResult.issues.files)) {
      console.log(`unused file ${filePath}`);
    }
    for (const [filePath, exports] of Object.entries(analysisResult.issues.exports)) {
      for (const issue of Object.values(exports)) {
        console.log(`unused export ${filePath}:${issue.line}:${issue.col} ${issue.symbol}`);
      }
    }
  }
}

if (shouldFix) {
  process.exit(0);
}

if (command === 'doctor') {
  process.exit((result as DoctorResult).warnings.length ? 1 : 0);
}

const analysisResult = result as AnalysisResult;
process.exit(analysisResult.counters.files || analysisResult.counters.exports || analysisResult.counters.unresolved ? 1 : 0);
