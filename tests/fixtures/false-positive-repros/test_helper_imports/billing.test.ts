import { formatPrice } from './billing';
import { makePrice } from './factory';

console.log(formatPrice(makePrice()));
