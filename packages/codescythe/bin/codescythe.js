#!/usr/bin/env node
'use strict';

const { analyze, fix } = require('@perplexity/codescythe');

const args = process.argv.slice(2);
const options = {};
let json = false;
let shouldFix = false;

for (let i = 0; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--json') {
    json = true;
  } else if (arg === '--fix') {
    shouldFix = true;
  } else if (arg === '--config' || arg === '-c') {
    options.config = args[++i];
  } else if (arg === '--directory' || arg === '-C') {
    options.cwd = args[++i];
  } else {
    console.error(`Unknown argument: ${arg}`);
    process.exit(2);
  }
}

const result = shouldFix ? fix(options) : analyze(options);
if (json) {
  console.log(JSON.stringify(result, null, 2));
} else if (shouldFix) {
  console.log(`Removed ${result.removedExports} unused exports from ${result.changedFiles.length} files`);
} else if (result.counters.files === 0 && result.counters.exports === 0 && result.counters.unresolved === 0) {
  console.log('No dead TypeScript code found');
} else {
  for (const path of Object.keys(result.issues.files)) {
    console.log(`unused file ${path}`);
  }
  for (const [path, exports] of Object.entries(result.issues.exports)) {
    for (const issue of Object.values(exports)) {
      console.log(`unused export ${path}:${issue.line}:${issue.col} ${issue.symbol}`);
    }
  }
}

process.exit(result.counters.files || result.counters.exports || result.counters.unresolved ? 1 : 0);
