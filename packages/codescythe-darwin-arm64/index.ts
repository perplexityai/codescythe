import {createRequire} from 'node:module';

type NativeBinding = {
  analyze(options: unknown): string;
  doctor(options: unknown): string;
  fix(options: unknown): string;
};

const require = createRequire(import.meta.url);
const native = require('./codescythe.darwin-arm64.node') as NativeBinding;

export const analyze = native.analyze;
export const doctor = native.doctor;
export const fix = native.fix;
export default native;
