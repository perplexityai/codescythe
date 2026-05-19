# Codescythe false-positive repros

While replacing Knip with `codescythe 0.3.0`, I found three patterns that look
like false positives. Each fixture in this package should report no unused files
or exports.

## Expected

All repro tests exit `0` with no dead-code findings.

## Original failures

1. CommonJS destructured `require`
   - `const { makeValue } = require("./make-value")`
   - Reports `make-value.js` as an unused file.
2. Dynamic import with destructured namespace
   - `import("./lazy").then(({ lazyValue }) => ...)`
   - Reports `lazy.ts` export `lazyValue` as unused.
3. Vite `import.meta.glob`
   - `import.meta.glob("./routes/*.ts", { eager: true })`
   - Reports matched route files like `routes/home.ts` as unused.

## Bazel repro tests

```starlark
load("//tests/bazel:codescythe_test.bzl", "codescythe_test", "source_group")

source_group(
    name = "commonjs_require_sources",
    srcs = [
        "commonjs_require/index.js",
        "commonjs_require/make-value.js",
    ],
)

codescythe_test(
    name = "commonjs_require_false_positive_repro",
    config = "commonjs_require/codescythe.json",
    expected_exit_code = 0,
    must_not_contain = ["make-value.js"],
    targets = [":commonjs_require_sources"],
)

source_group(
    name = "dynamic_import_destructuring_sources",
    srcs = [
        "dynamic_import_destructuring/index.ts",
        "dynamic_import_destructuring/lazy.ts",
    ],
)

codescythe_test(
    name = "dynamic_import_destructuring_false_positive_repro",
    config = "dynamic_import_destructuring/codescythe.json",
    expected_exit_code = 0,
    must_not_contain = ["lazyValue"],
    targets = [":dynamic_import_destructuring_sources"],
)

source_group(
    name = "import_meta_glob_sources",
    srcs = [
        "import_meta_glob/index.ts",
        "import_meta_glob/routes/home.ts",
    ],
)

codescythe_test(
    name = "import_meta_glob_false_positive_repro",
    config = "import_meta_glob/codescythe.json",
    expected_exit_code = 0,
    must_not_contain = ["routes/home.ts"],
    targets = [":import_meta_glob_sources"],
)
```

## Fixture contents

### `commonjs_require/codescythe.json`

```json
{
  "entry": "index.js",
  "project": ["**/*.js"],
  "unresolvedImports": {
    "mode": "ignore"
  },
  "ignoreExportsUsedInFile": true
}
```

### `commonjs_require/index.js`

```js
const { makeValue } = require("./make-value");

console.log(makeValue());
```

### `commonjs_require/make-value.js`

```js
module.exports = {
  makeValue() {
    return "from-commonjs";
  },
};
```

### `dynamic_import_destructuring/codescythe.json`

```json
{
  "entry": "index.ts",
  "project": ["**/*.ts"],
  "unresolvedImports": {
    "mode": "ignore"
  },
  "ignoreExportsUsedInFile": true
}
```

### `dynamic_import_destructuring/index.ts`

```ts
void import("./lazy").then(({ lazyValue }) => {
  console.log(lazyValue);
});
```

### `dynamic_import_destructuring/lazy.ts`

```ts
export const lazyValue = "loaded";
```

### `import_meta_glob/codescythe.json`

```json
{
  "entry": "index.ts",
  "project": ["**/*.ts"],
  "unresolvedImports": {
    "mode": "ignore"
  },
  "ignoreExportsUsedInFile": true
}
```

### `import_meta_glob/index.ts`

```ts
const modules = import.meta.glob("./routes/*.ts", { eager: true });

console.log(Object.keys(modules));
```

### `import_meta_glob/routes/home.ts`

```ts
export const route = {
  path: "/home",
};
```
