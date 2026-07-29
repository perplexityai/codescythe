# Ideas

## Declared lazy-boundary enforcement

`import-conflicts` finds modules reached through both runtime-static and dynamic
paths from the same entrypoint. It cannot identify every module that should be
lazy: whether a modal, panel, or specialized renderer should load on activation
depends on product behavior and callsite UX.

Add an opt-in contract for boundaries whose lazy intent is already known:

```ts
// codescythe-require-lazy -- expensive structured-answer dispatcher
const StructuredAnswer = Dynamic(async () => import("./StructuredAnswer"));
```

For each declaration, Codescythe should:

- resolve the dynamic target;
- find every configured entrypoint that can reach the dynamic importer;
- fail when any of those entrypoints can also reach the target through a
  runtime-static path;
- print every violating entrypoint, static importer, and shortest proof path;
- ignore type-only imports because they do not affect bundling;
- fail clearly when the annotated statement no longer contains a supported
  dynamic import.

This separates discovery from enforcement. Bundle analysis and human judgment
find worthwhile boundaries. Codescythe keeps those boundaries detached after
they are declared.

### Non-goals

- Inferring that a UI surface is rarely opened.
- Deciding whether loading latency is acceptable.
- Estimating emitted or transferred bundle bytes.
- Treating every static import of a dynamically imported module as invalid
  across unrelated entrypoint graphs.

### Open questions

- Should configuration support the same contract for generated callsites?
- Should one annotation cover every dynamic import in a statement?
- Should violations support reasoned exceptions, or require narrowing the
  declared entrypoint scope?
