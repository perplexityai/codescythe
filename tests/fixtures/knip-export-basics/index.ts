import { myExport } from './my-module.js';
import type { UsedType } from './types';

type Local = UsedType;

export const ignoredExportInEntryFile: Local = myExport;
