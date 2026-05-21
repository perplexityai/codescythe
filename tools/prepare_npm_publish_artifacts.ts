'use strict';

const childProcess = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

type PackageSpec = {
  packageDir: string;
  artifactDir: string;
  files: string[];
  native?: {
    target: string;
    filename: string;
  };
};

const specs: PackageSpec[] = [
  {
    packageDir: 'packages/codescythe',
    artifactDir: 'codescythe',
    files: ['README.md', 'index.d.ts', 'index.ts', 'package.json', 'bin'],
  },
  {
    packageDir: 'packages/codescythe-darwin-arm64',
    artifactDir: 'codescythe-darwin-arm64',
    files: ['README.md', 'index.ts', 'package.json'],
    native: {
      target: '//packages/codescythe-darwin-arm64:native_node',
      filename: 'codescythe.darwin-arm64.node',
    },
  },
  {
    packageDir: 'packages/codescythe-linux-amd64',
    artifactDir: 'codescythe-linux-amd64',
    files: ['README.md', 'index.ts', 'package.json'],
    native: {
      target: '//packages/codescythe-linux-amd64:native_node',
      filename: 'codescythe.linux-amd64.node',
    },
  },
  {
    packageDir: 'packages/codescythe-linux-arm64',
    artifactDir: 'codescythe-linux-arm64',
    files: ['README.md', 'index.ts', 'package.json'],
    native: {
      target: '//packages/codescythe-linux-arm64:native_node',
      filename: 'codescythe.linux-arm64.node',
    },
  },
];

const repoRoot = process.cwd();
const artifactsRoot = path.resolve(repoRoot, process.argv[2] ?? 'artifacts');

fs.rmSync(artifactsRoot, {force: true, recursive: true});
fs.mkdirSync(artifactsRoot, {recursive: true});

for (const spec of specs) {
  const sourceDir = path.join(repoRoot, spec.packageDir);
  const outputDir = path.join(artifactsRoot, spec.artifactDir);
  fs.mkdirSync(outputDir, {recursive: true});

  for (const file of spec.files) {
    copyPath(path.join(sourceDir, file), path.join(outputDir, file));
  }

  rewritePackageJson(path.join(outputDir, 'package.json'));

  if (spec.native) {
    const nativeOutput = bazelOutput(spec.native.target);
    copyPath(nativeOutput, path.join(outputDir, spec.native.filename));
  }
}

function copyPath(source: string, destination: string) {
  fs.cpSync(source, destination, {recursive: true});
}

function rewritePackageJson(packageJsonPath: string) {
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8')) as {
    version: string;
    optionalDependencies?: Record<string, string>;
  };

  for (const dependencyName of Object.keys(packageJson.optionalDependencies ?? {})) {
    if (dependencyName.startsWith('@perplexity/codescythe-')) {
      packageJson.optionalDependencies![dependencyName] = packageJson.version;
    }
  }

  fs.writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

function bazelOutput(target: string): string {
  const output = childProcess.execFileSync('bazel', ['cquery', '--output=files', target], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  const files = output
    .split('\n')
    .map((line: string) => line.trim())
    .filter(Boolean);

  if (files.length === 0) {
    throw new Error(`bazel cquery returned no files for ${target}`);
  }

  return path.resolve(repoRoot, files[files.length - 1]);
}
