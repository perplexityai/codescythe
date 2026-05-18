import { runtime } from '#app/runtime';
import { client } from '#bazel_generated/client';
import '#pplx_generated/api/foo';
import './missing';

console.log(runtime, client);
