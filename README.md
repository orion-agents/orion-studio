> [!IMPORTANT]
> Remove this line to confirm you've reviewed this PR before submitting.

# Orion Studio

[![CI](https://github.com/orion-agents/orion-studio/actions/workflows/run_tests.yml/badge.svg)](https://github.com/orion-agents/orion-studio/actions/workflows/run_tests.yml)

> [!IMPORTANT]
> Orion Studio v1.17.0 is released for **macOS 11.0 or later on Apple Silicon
> (arm64)**. The AI Native workspace is available as an initial, optional
> conversation-first layout and will continue to receive UI refinements.

Orion Studio is an open-source code editor derived from
[Zed](https://github.com/zed-industries/zed) and its GPUI foundation. Zed and
Zed Industries are referenced for upstream code and attribution; Zed's hosted
services and legal policies are not Orion Studio services or policies.

## Download Orion Studio

Download Orion Studio only from
[GitHub Releases](https://github.com/orion-agents/orion-studio/releases). For a
Stable release, select an exact version tag such as `v1.17.0` (not a release
marked **Pre-release**) and download `Orion-Studio-aarch64.dmg`.

GitHub Releases is the only supported download channel. There is currently no
package manager installation, Intel Mac build, Linux build, or Windows build.
Do not install an artifact if its GitHub release does not publish a SHA-256
digest or if macOS Gatekeeper rejects the installed app.

Before installing, read:

- [macOS Apple Silicon installation](./docs/installation.md)
- [release policy and trust gates](./docs/releases/release-policy.md)
- [Preview scope, migration notes, and known limitations](./docs/releases/preview-macos-arm64.md)
  when evaluating a pre-release build

## Product Boundaries

- Orion Studio does not offer an Orion-operated account, subscription, or
  hosted online service as part of the current distribution.
- An Orion extension marketplace is not currently supported.
- Automatic updates are not supported; install later versions manually
  from GitHub Releases and verify each new artifact.
- Inherited or user-configured third-party integrations may have their own
  availability, data handling, and terms. Do not assume they are operated or
  supported by Orion Studio.
- After the first workspace frame appears, Orion Studio may copy legacy Zed
  configuration and application data in the background. Quit Zed and make a
  backup before launching; restart Orion Studio after a successful import.

## Developing Orion Studio

- [Build on macOS](./docs/src/development/macos.md)
- [Build on Linux](./docs/src/development/linux.md)
- [Build on Windows](./docs/src/development/windows.md)

These are source-development guides. They do not mean downloadable
packages are available for every development platform.

## Contributing and Support

See [CONTRIBUTING.md](./CONTRIBUTING.md) before proposing a change. For release
help or reproducible bugs, use [SUPPORT.md](./SUPPORT.md). Report security
vulnerabilities privately as described in [SECURITY.md](./SECURITY.md).

## Licensing and Upstream Attribution

Orion Studio source code is licensed primarily under
[GPL-3.0-or-later](./LICENSE-GPL), with
[Apache-2.0](./LICENSE-APACHE) components where marked. Third-party notices are
generated during packaging and included in the application bundle.

This fork preserves attribution to Zed Industries, Inc. and other upstream
contributors. The presence of upstream code or names does not make Zed's
privacy policy, terms of service, subprocessor list, sponsorship program, or
support channels applicable to Orion Studio.
