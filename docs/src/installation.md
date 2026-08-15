---
title: Install Orion Studio - macOS, Linux, Windows
description: Build or install Orion Studio on macOS, Linux, Windows, and FreeBSD without relying on undeployed Orion services.
---

# Installing Orion Studio

## Release availability

Orion Studio does not assume that a public download, update, Homebrew, Winget,
Flatpak, or Snap service has been deployed. The repository is the source of
truth. Build from source using the platform guides below.

If maintainers publish signed artifacts, they will appear on the
[GitHub Releases page](https://github.com/orion-agents/orion-studio/releases).
Verify the release notes, checksums, signatures, architecture, and channel
before installing. An empty release page means no public build is available.

Do not use Zed-branded package identifiers as a substitute: they install the
upstream Zed product, not Orion Studio.

## Build from source

- [macOS](./development/macos.md)
- [Linux](./development/linux.md)
- [Windows](./development/windows.md)
- [FreeBSD](./development/freebsd.md)

The application package and main binary are named `orion-studio`. Release
bundles use `orion` as the preferred CLI launcher and may expose
`orion-studio` as an equivalent launcher. See the [CLI reference](./reference/cli.md).

## Install a locally built Linux bundle

After producing `orion-studio-linux-<arch>.tar.gz`, run the repository installer
with an explicit local bundle path:

```sh
ORION_STUDIO_BUNDLE_PATH=/absolute/path/to/orion-studio-linux-x86_64.tar.gz \
  ./script/install.sh
```

This avoids the script's network download path. A hosted installer URL may be
documented only after the corresponding Orion release service is deployed and
verified. To remove an installation created by the script, run
`orion --uninstall`.

## System Requirements

### macOS

Orion Studio supports the following macOS releases:

| Version       | Codename | Apple Status   | Orion Studio Status |
| ------------- | -------- | -------------- | ------------------- |
| macOS 26.x    | Tahoe    | Supported      | Supported           |
| macOS 15.x    | Sequoia  | Supported      | Supported           |
| macOS 14.x    | Sonoma   | Supported      | Supported           |
| macOS 13.x    | Ventura  | Supported      | Supported           |
| macOS 12.x    | Monterey | EOL 2024-09-16 | Supported           |
| macOS 11.x    | Big Sur  | EOL 2023-09-26 | Partially Supported |
| macOS 10.15.x | Catalina | EOL 2022-09-12 | Partially Supported |

The macOS releases labelled "Partially Supported" (Big Sur and Catalina) do not support screen sharing when Orion Studio is connected to an operator-deployed collaboration service. This feature uses the [LiveKit SDK](https://livekit.io), which relies on [ScreenCaptureKit.framework](https://developer.apple.com/documentation/screencapturekit/) available only on macOS 12 (Monterey) and newer.

#### Mac Hardware

Orion Studio supports machines with Intel (x86_64) or Apple (aarch64) processors that meet the above macOS requirements:

- MacBook Pro (Early 2015 and newer)
- MacBook Air (Early 2015 and newer)
- MacBook (Early 2016 and newer)
- Mac Mini (Late 2014 and newer)
- Mac Pro (Late 2013 or newer)
- iMac (Late 2015 and newer)
- iMac Pro (all models)
- Mac Studio (all models)

### Linux

Orion Studio supports 64-bit Intel/AMD (x86_64) and 64-bit Arm (aarch64) processors.

Orion Studio requires a Vulkan 1.3 driver and the following desktop portals:

- `org.freedesktop.portal.FileChooser`
- `org.freedesktop.portal.OpenURI`
- `org.freedesktop.portal.Secret` or `org.freedesktop.Secrets`

### Windows

Orion Studio supports the following Windows releases:

| Version                            | Orion Studio Status |
| ---------------------------------- | ------------------- |
| Windows 11, version 22H2 and later | Supported           |
| Windows 10, version 1903 and later | Supported           |

A 64-bit operating system is required to run Orion Studio.

#### Windows Hardware

Orion Studio supports machines with x64 (Intel, AMD) or Arm64 (Qualcomm) processors that meet the following requirements:

- Graphics: A GPU that supports DirectX 11 (most PCs from 2012+).
- Driver: Current NVIDIA/AMD/Intel/Qualcomm driver (not the Microsoft Basic Display Adapter).

### FreeBSD

Not yet available as an official download. Can be built [from source](./development/freebsd.md).

### Web

Not supported at this time. See our [Platform Support issue](https://github.com/orion-agents/orion-studio/issues/5391).
