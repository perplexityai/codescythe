import { createRequire } from 'node:module';

type AnalyzeOptions = {
  config?: string;
  cwd?: string;
  explainExport?: string;
  force?: boolean;
  verbose?: boolean;
};

type NativeBinding = {
  analyze(options: AnalyzeOptions): string;
  doctor(options: AnalyzeOptions): string;
  fix(options: AnalyzeOptions): string;
};

const require = createRequire(import.meta.url);
const native = requireNative();

export function analyze(options: AnalyzeOptions = {}) {
  return JSON.parse(native.analyze(options));
}

export function fix(options: AnalyzeOptions = {}) {
  return JSON.parse(native.fix(options));
}

export function doctor(options: AnalyzeOptions = {}) {
  return JSON.parse(native.doctor(options));
}

export {native};

function requireNative(): NativeBinding {
  const {nativeFile, packageName} = nativePackageTarget();
  try {
    return require(`${packageName}/${nativeFile}`) as NativeBinding;
  } catch (error: unknown) {
    const hint = `Codescythe native package ${packageName} is not installed for ${process.platform}/${process.arch}.`;
    if (error instanceof Error) {
      error.message = `${hint}\n${error.message}`;
    }
    throw error;
  }
}

function nativePackageTarget(): {nativeFile: string; packageName: string} {
  if (process.platform === 'linux' && process.arch === 'x64') {
    return {
      nativeFile: 'codescythe.linux-amd64.node',
      packageName: 'codescythe-linux-amd64',
    };
  }
  if (process.platform === 'linux' && process.arch === 'arm64') {
    return {
      nativeFile: 'codescythe.linux-arm64.node',
      packageName: 'codescythe-linux-arm64',
    };
  }
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    return {
      nativeFile: 'codescythe.darwin-arm64.node',
      packageName: 'codescythe-darwin-arm64',
    };
  }
  throw new Error(`Codescythe does not ship a native package for ${process.platform}/${process.arch}`);
}

export default {
  analyze,
  doctor,
  fix,
  native,
};
