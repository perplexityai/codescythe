# Changelog

## [0.10.0](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.9.0...codescythe_cli_v0.10.0) (2026-07-30)


### Features

* add unused-file expectations ([#108](https://github.com/perplexityai/codescythe/issues/108)) ([7496faf](https://github.com/perplexityai/codescythe/commit/7496faf96edaa22ee08e639de7a86970a19c8540))

## [0.9.0](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.8.0...codescythe_cli_v0.9.0) (2026-07-29)


### Features

* **query:** report alternate conflict entrypoints ([#106](https://github.com/perplexityai/codescythe/issues/106)) ([bf27ae9](https://github.com/perplexityai/codescythe/commit/bf27ae913ddeb9e0aadba6dc0a2ec7393f42791d))
* **query:** suppress intentional import conflicts ([#103](https://github.com/perplexityai/codescythe/issues/103)) ([c351f4c](https://github.com/perplexityai/codescythe/commit/c351f4c27433529c0d679cccfbfe73b05a2e9dcf))
* **query:** suppress intentional preload subtrees ([#105](https://github.com/perplexityai/codescythe/issues/105)) ([456ee1a](https://github.com/perplexityai/codescythe/commit/456ee1ae1928a20deb5dc8a356ec76ad28e595c6))


### Bug Fixes

* **query:** ignore type-only re-exports in conflicts ([#101](https://github.com/perplexityai/codescythe/issues/101)) ([7613018](https://github.com/perplexityai/codescythe/commit/7613018c39eaba1a317494d25e36048e40b2ef72))

## [0.8.0](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.7.1...codescythe_cli_v0.8.0) (2026-07-28)


### Features

* **query:** detect import conflicts ([#100](https://github.com/perplexityai/codescythe/issues/100)) ([687cb17](https://github.com/perplexityai/codescythe/commit/687cb17a1c9686201e7fa4eefe7ea3d1cba35c44))
* **query:** distinguish type-only imports ([#99](https://github.com/perplexityai/codescythe/issues/99)) ([52d879d](https://github.com/perplexityai/codescythe/commit/52d879d2b2de4e20311776089c4a97f16e0aa4c4))


### Bug Fixes

* **query:** distinguish dynamic imports ([#97](https://github.com/perplexityai/codescythe/issues/97)) ([b44ac51](https://github.com/perplexityai/codescythe/commit/b44ac51c6a8d106bd2baf5a2d22537ee5e25730d))

## [0.7.1](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.7.0...codescythe_cli_v0.7.1) (2026-07-27)


### Bug Fixes

* enforce case-sensitive module resolution ([#95](https://github.com/perplexityai/codescythe/issues/95)) ([e1dd1c4](https://github.com/perplexityai/codescythe/commit/e1dd1c4ab2c5eaec5a2ce31aa89d293fd21dd55e))

## [0.7.0](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.6.1...codescythe_cli_v0.7.0) (2026-06-01)


### Features

* **cli:** add query diagnostics ([#92](https://github.com/perplexityai/codescythe/issues/92)) ([718688f](https://github.com/perplexityai/codescythe/commit/718688ffebb2027bd437db050cf305a2ed410015))
* **cli:** add query unresolved filter ([#90](https://github.com/perplexityai/codescythe/issues/90)) ([9a7aa33](https://github.com/perplexityai/codescythe/commit/9a7aa3320fdcfb924943779ede8408e2017461e9))
* **cli:** scope query unresolved diagnostics ([#93](https://github.com/perplexityai/codescythe/issues/93)) ([4b79c85](https://github.com/perplexityai/codescythe/commit/4b79c85478a6a1c71da6aeeaf06e99599a6eb3ea))

## [0.6.1](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.6.0...codescythe_cli_v0.6.1) (2026-05-30)


### Bug Fixes

* skip PR title check on push ([#88](https://github.com/perplexityai/codescythe/issues/88)) ([6830ca0](https://github.com/perplexityai/codescythe/commit/6830ca061a4394203245184f416245ad0bc9c89f))

## [0.6.0](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.5.0...codescythe_cli_v0.6.0) (2026-05-29)


### Features

* add dependency path query command ([#80](https://github.com/perplexityai/codescythe/issues/80)) ([70d4482](https://github.com/perplexityai/codescythe/commit/70d4482ecd2456a03cd503c3762c553f4f3e5284))
* merge somepath query modes ([#85](https://github.com/perplexityai/codescythe/issues/85)) ([eeb0bb6](https://github.com/perplexityai/codescythe/commit/eeb0bb60ab85c1512c81a79a7ceea5704a5302a8))
* render query output as svg ([#83](https://github.com/perplexityai/codescythe/issues/83)) ([aeb7974](https://github.com/perplexityai/codescythe/commit/aeb797422dccdfb18d62df48c861fe927f09e973))
* render query results as mermaid ([#81](https://github.com/perplexityai/codescythe/issues/81)) ([b6667d9](https://github.com/perplexityai/codescythe/commit/b6667d92cf80ceb8eb69d2194c1330614354c668))

## [0.5.0](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.4.15...codescythe_cli_v0.5.0) (2026-05-27)


### Features

* support [@internal](https://github.com/internal) test usage ([#78](https://github.com/perplexityai/codescythe/issues/78)) ([2900d09](https://github.com/perplexityai/codescythe/commit/2900d093b1659a10a89945a44cf9e6dafecb7a67))

## [0.4.15](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.4.14...codescythe_cli_v0.4.15) (2026-05-27)


### Bug Fixes

* **npm:** restore bin mode after dist download ([#76](https://github.com/perplexityai/codescythe/issues/76)) ([fa3d1d6](https://github.com/perplexityai/codescythe/commit/fa3d1d616ce5a83947ad9453914bf5008525a33a))

## [0.4.14](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.4.13...codescythe_cli_v0.4.14) (2026-05-27)


### Bug Fixes

* **npm:** publish ESM JS dist packages ([#73](https://github.com/perplexityai/codescythe/issues/73)) ([4c5d492](https://github.com/perplexityai/codescythe/commit/4c5d49264a3ce707baf332d72fc13a2085155fbd))

## [0.4.13](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.4.12...codescythe_cli_v0.4.13) (2026-05-26)


### Bug Fixes

* prepare public registry publishing ([#60](https://github.com/perplexityai/codescythe/issues/60)) ([064ef29](https://github.com/perplexityai/codescythe/commit/064ef29fb0bad2a4218bdd5502811518c9c7c9ab))


### Performance Improvements

* dedupe import resolution ([#58](https://github.com/perplexityai/codescythe/issues/58)) ([64bbdb0](https://github.com/perplexityai/codescythe/commit/64bbdb055df3222c26a06f776870d5e891dd5426))
* memoize import resolution ([#54](https://github.com/perplexityai/codescythe/issues/54)) ([6cdf3c9](https://github.com/perplexityai/codescythe/commit/6cdf3c93d46ea784bfb6b4f6e1869eba681fede0))

## [0.4.12](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.4.11...codescythe_cli_v0.4.12) (2026-05-22)


### Bug Fixes

* keep node_modules resolver metadata visible ([#51](https://github.com/perplexityai/codescythe/issues/51)) ([f07f1ba](https://github.com/perplexityai/codescythe/commit/f07f1bab5380e0e0b9a24e2b9b26371faa2fc1d7))

## [0.4.11](https://github.com/perplexityai/codescythe/compare/codescythe_cli_v0.4.10...codescythe_cli_v0.4.11) (2026-05-21)


### Bug Fixes

* honor ignored resolver metadata ([#48](https://github.com/perplexityai/codescythe/issues/48)) ([a2d86a9](https://github.com/perplexityai/codescythe/commit/a2d86a961f3d076396bf90e599438680700b2ad8))
* stamp analyzer diagnostics version ([#46](https://github.com/perplexityai/codescythe/issues/46)) ([4ac2a52](https://github.com/perplexityai/codescythe/commit/4ac2a526591350ef7bddb34f26b35a6387912779))
