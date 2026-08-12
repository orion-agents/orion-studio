# Orion Studio

[![Orion Studio](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/orion-agents/orion-studio/main/assets/badge/v0.json)](https://orion.dev)
[![CI](https://github.com/orion-agents/orion-studio/actions/workflows/run_tests.yml/badge.svg)](https://github.com/orion-agents/orion-studio/actions/workflows/run_tests.yml)

Welcome to Orion Studio, a high-performance, multiplayer code editor from the creators of [Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

Orion Studio is a fork of [Zed](https://zed.dev), built on the open-source GPUI framework and the work of Zed Industries, Inc.

---

### Installation

On macOS, Linux, and Windows you can [download Orion Studio directly](https://orion.dev/download) or install Orion Studio via your local package manager ([macOS](https://orion.dev/docs/installation#macos)/[Linux](https://orion.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://orion.dev/docs/windows#package-managers)).

Other platforms are not yet available:

- Web ([tracking discussion](https://github.com/orion-agents/orion-studio/discussions/26195))

### Developing Orion Studio

- [Building Orion Studio for macOS](./docs/src/development/macos.md)
- [Building Orion Studio for Linux](./docs/src/development/linux.md)
- [Building Orion Studio for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Orion Studio.

Also... we're hiring! Check out our [jobs](https://orion.dev/jobs) page for open roles.

### Licensing

Orion Studio source code is licensed primarily under GPL-3.0-or-later, with Apache-2.0 components where marked. This preserves the upstream licensing of the Zed project from which this fork derives.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your crate's Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorship

Orion Studio is derived from the work of **Zed Industries, Inc.**, a for-profit company, and is released under the same open-source licenses.

If you'd like to financially support the upstream project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.
