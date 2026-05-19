import { live } from './live';
import { used } from './module';
import { usedNamespace } from './namespace';
import type { UsedType } from './types';

const typed: UsedType = { value: usedNamespace };

console.log(live, used, usedNamespace, typed);
