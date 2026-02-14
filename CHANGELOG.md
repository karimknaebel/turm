# Changelog

## [0.13.0](https://github.com/karimknaebel/turm/compare/v0.12.0...v0.13.0) (2026-02-14)


### Features

* add mouse support ([85bb9eb](https://github.com/karimknaebel/turm/commit/85bb9eb710a98281631a4e67ba11bd1f89e3abbd))

## [0.12.0](https://github.com/karimknaebel/turm/compare/v0.11.0...v0.12.0) (2026-02-10)


### Features

* add job name to detail window ([#42](https://github.com/karimknaebel/turm/issues/42)) ([0e40ab4](https://github.com/karimknaebel/turm/commit/0e40ab486bea2630f1717cc31e6380446ff0b87f))

## [0.11.0](https://github.com/karimknaebel/turm/compare/v0.10.0...v0.11.0) (2025-12-23)


### Features

* add g and G bindings to jump to first/last job ([2b92a8e](https://github.com/karimknaebel/turm/commit/2b92a8edb426dbec2c52e26127d8516a2e116dd2))
* scroll job list with ctrl+d/u ([3fe52ea](https://github.com/karimknaebel/turm/commit/3fe52eacc9b8745456e382281b1c58c1d21792e2))


### Bug Fixes

* clean up jobs refresh ([cc7b29b](https://github.com/karimknaebel/turm/commit/cc7b29b2f17c13b182c2171911da756c5bfc549b))
* clippy ([8cc6246](https://github.com/karimknaebel/turm/commit/8cc624612948bc0e57a5a14d73ac0f78c6040bdf))
* clippy ([6644393](https://github.com/karimknaebel/turm/commit/6644393ae45c50438e6315e6efd5676ad0680786))
* format ([bb28b2b](https://github.com/karimknaebel/turm/commit/bb28b2be8df5e53803a16847c1ce9c59f4458399))
* job list height ([9fdd3d7](https://github.com/karimknaebel/turm/commit/9fdd3d736d5ef65a72bf33da0103e952303dd574))
* preserve job selection across refresh ([e54ac01](https://github.com/karimknaebel/turm/commit/e54ac016bd844b545d53bccf1480a38214a690ed))
* remove unnecessary job_list_state modification in ui func ([50a780c](https://github.com/karimknaebel/turm/commit/50a780c967b41459dcb81702a2c28c0065e4d44d))
* reset terminal on panic ([bc1de57](https://github.com/karimknaebel/turm/commit/bc1de573353ab9cc19d123da1565b79e747da1c3)), closes [#52](https://github.com/karimknaebel/turm/issues/52)
* revert calling scancel inside of thread ([741198f](https://github.com/karimknaebel/turm/commit/741198f5a10724b74e7f094743ae4d7a4ee1abed))
* wait for scancel to finish ([8b11875](https://github.com/karimknaebel/turm/commit/8b1187543997c9b03e88a92d6183a0eb7f3b2edd))

## [0.10.0](https://github.com/karimknaebel/turm/compare/v0.9.0...v0.10.0) (2025-12-12)


### Features

* rounded corners ([a689259](https://github.com/karimknaebel/turm/commit/a6892592723c50d2d7ce48b9379b497a573c3a68))

## [0.9.0](https://github.com/karimknaebel/turm/compare/v0.8.0...v0.9.0) (2025-08-26)


### Features

* auto-select first job on start ([c39613c](https://github.com/karimknaebel/turm/commit/c39613c32ca18625807a56081655f14533d63d32))
* show estimated start time for pending jobs ([6ce5f01](https://github.com/karimknaebel/turm/commit/6ce5f01730d400f1a9be0d97dd17100728ce72d0))


### Bug Fixes

* cargo warning ([8979926](https://github.com/karimknaebel/turm/commit/8979926149d07b7e78e2839d767679efb4d52c2b))

## [0.8.0](https://github.com/karimknaebel/turm/compare/v0.7.3...v0.8.0) (2025-08-24)


### Features

* auto-refresh non-existing file paths until they are created ([3a3acc6](https://github.com/karimknaebel/turm/commit/3a3acc6480faf58dc93de31d031bd2a90db117e8))

## [0.7.3](https://github.com/karimknaebel/turm/compare/v0.7.2...v0.7.3) (2024-07-28)


### Miscellaneous Chores

* release 0.7.3 ([ae8665b](https://github.com/karimknaebel/turm/commit/ae8665b25d68842dc1100f85aee643bc122ef52f))

## [0.7.2](https://github.com/karimknaebel/turm/compare/v0.7.1...v0.7.2) (2024-07-28)


### Bug Fixes

* crash on resize ([96f4f16](https://github.com/karimknaebel/turm/commit/96f4f1683ee98547dadc610cf21f293858ba9d50))

## [0.7.1](https://github.com/karimknaebel/turm/compare/v0.6.0...v0.7.1) (2024-07-28)


### Features

* pretty text wrapping ([51dc964](https://github.com/karimknaebel/turm/commit/51dc9645f506b89a0444db64cab6ddc0d2ecdaf0))
* toggle log text wrapping. update deps ([5243a36](https://github.com/karimknaebel/turm/commit/5243a368c173070c58ce8a51bce56be9f916ec21))
* truncated line indicator ([f347664](https://github.com/karimknaebel/turm/commit/f347664ecd94db785140eac296e37d66e203a81b))


### Bug Fixes

* correctly resolve relative log file paths ([0ecc902](https://github.com/karimknaebel/turm/commit/0ecc902f036244ed67d29eb686dcbf2c413ec51c))
* crash on resize ([6dc3b1d](https://github.com/karimknaebel/turm/commit/6dc3b1d9f387d3b2accdc34b7e8a0c42995424c9))


### Miscellaneous Chores

* release 0.7.1 ([499a3a6](https://github.com/karimknaebel/turm/commit/499a3a69059adab68444d552acc4838962db4e0b))

## [0.6.0](https://github.com/karimknaebel/turm/compare/v0.5.0...v0.6.0) (2023-09-23)


### Features

* toggle stdout/stderr ([bcd773b](https://github.com/karimknaebel/turm/commit/bcd773bd21ccb64860e651e2da881d57253fecb8))

## [0.5.0](https://github.com/karimknaebel/turm/compare/v0.4.0...v0.5.0) (2023-09-15)


### Features

* show job count ([c169e18](https://github.com/karimknaebel/turm/commit/c169e1844574885246736dbde920ae0f77b121b2))

## [0.4.0](https://github.com/karimknaebel/turm/compare/v0.3.0...v0.4.0) (2023-04-23)


### Features

* faster fast scrolling (shift/control/alt) ([37e205a](https://github.com/karimknaebel/turm/commit/37e205aaf819e99e13aea70327de84289cba0482))
* scroll to top/bottom ([0022a70](https://github.com/karimknaebel/turm/commit/0022a70a58d6a0f2b1e159f0b5afef99ae6ea2c1))

## [0.3.0](https://github.com/karimknaebel/turm/compare/v0.2.0...v0.3.0) (2023-04-17)


### Features

* add shell completions ([e9b8de0](https://github.com/karimknaebel/turm/commit/e9b8de0018b3dd91d72db6e3c164aa18a1fe17d9))
* proper cli with help ([90988f6](https://github.com/karimknaebel/turm/commit/90988f65208b353204acd6a570be45e753bfcdfc))

## [0.2.0](https://github.com/karimknaebel/turm/compare/v0.1.0...v0.2.0) (2023-04-15)


### Features

* cancel jobs ([bc05723](https://github.com/karimknaebel/turm/commit/bc057230244ce215a585dbb318de762913524a5b))
* select first job on launch ([7c742fd](https://github.com/karimknaebel/turm/commit/7c742fdd3b66787b10df6a017de6c7522c8f9858))


### Bug Fixes

* clear the log on empty selection ([518afdb](https://github.com/karimknaebel/turm/commit/518afdbf67ada9ea1d7b2597765630cba8a00ee4))
* correctly display job ids in arrays ([bc05723](https://github.com/karimknaebel/turm/commit/bc057230244ce215a585dbb318de762913524a5b))

## 0.1.0 (2023-03-31)


### Features

* accept same cli args as `squeue` ([1f1a5ac](https://github.com/karimknaebel/turm/commit/1f1a5ac8f0b92b435b09e09981c95cbb00290a20))
* add cargo metadata ([78487bb](https://github.com/karimknaebel/turm/commit/78487bbe93c8c1efaef8b218e72c68a4dbe3c67a))
* better error handling ([ad47d19](https://github.com/karimknaebel/turm/commit/ad47d19ad6abccb80bc7d5c9ac3faf44ca03a92a))
* better layout ([67e24e0](https://github.com/karimknaebel/turm/commit/67e24e078df0eed492123e498282942400cbbcf9))
* config interval file ([7e6678d](https://github.com/karimknaebel/turm/commit/7e6678d834ce5535dfe2ede8e88974ccbf36c453))
* fast scroll ([8df9158](https://github.com/karimknaebel/turm/commit/8df91589f8ef6c3cd403faecfc40142fd238d0a4))
* faster log file loading ([9f954cc](https://github.com/karimknaebel/turm/commit/9f954ccff53fc7ffdb4412d1a490ef012bf4cc95))
* faster log loading ([b4b0fa4](https://github.com/karimknaebel/turm/commit/b4b0fa4df97d51976f2cadffd527a07fd3804346))
* help bar ([ab63a9e](https://github.com/karimknaebel/turm/commit/ab63a9e2cd9b2ea05a8d45789b8dfb04d580c932))
* partial reads (like tail -f) ([86f04af](https://github.com/karimknaebel/turm/commit/86f04af1bf78783c37c4cecbef4d3292280f4f5e))
* prettier ([c70de5e](https://github.com/karimknaebel/turm/commit/c70de5ea4f412531c203bb308ee769e6cc861828))
* scroll to bottom with end ([01423f1](https://github.com/karimknaebel/turm/commit/01423f1a8c5da16f97dc01efd4e73cbb96d8c810))
* show job details ([904ff7c](https://github.com/karimknaebel/turm/commit/904ff7cef52e8971f7c6146ec217065988001336))
* show state and reason in details panel ([a77d4a3](https://github.com/karimknaebel/turm/commit/a77d4a3ff7d823f89ea33921dee28aa9ff7b6a3f))
* show state in list ([823a0a2](https://github.com/karimknaebel/turm/commit/823a0a263bc33b7a1e77d92601820059dfc22a14))


### Bug Fixes

* error on shutdown ([ff516ca](https://github.com/karimknaebel/turm/commit/ff516cac734fcd06a443122aca408d228046484a))
* hide incomplete lines in log files ([28eb452](https://github.com/karimknaebel/turm/commit/28eb452f9b4e8900d74be491368787bbe2197fc1))
* log title ([d42f79a](https://github.com/karimknaebel/turm/commit/d42f79ae7dcfec4d33d29fdcc48e1e986d1ea8b9))
* warnings ([ffd1211](https://github.com/karimknaebel/turm/commit/ffd1211228490960186d9cf8dc1d773a38558b16))
