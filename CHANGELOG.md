# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/cuibonobo/procreate-rs/releases/tag/v0.1.0) - 2026-03-31

### Added

- rename export/import to unpack/pack, add --help via clap
- Embed correct ICC profile into Procreate files
- Generate Procreate files from PNGs and manifest
- Include thumbnail with export
- Parse .procreate files into images for each layer

### Other

- add release-plz workflow
- add contributor tooling and contribution guide
- Apply rustfmt formatting across the codebase
- Add GitHub Actions CI workflow
- Move procreate-export binary to src/bin/ for symmetry with procreate-import
- Update with ICC profile findings and new capabilities
- New archive writer and import roundtrip integration tests
- Simplify and lint
- Add unit and integration tests for existing features
- Initial commit
