import { aliased } from '@/aliased';
import { internal } from '#internal';
import { extension } from './extension.js';
import externalDefault from 'external-pkg';
import path from 'node:path';

console.log(aliased, internal, extension, externalDefault, path.sep);
