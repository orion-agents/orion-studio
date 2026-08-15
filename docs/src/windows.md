---
title: Orion Studio on Windows
description: Build Orion Studio on Windows or install a verified published release artifact.
---

# Orion Studio on Windows

## Installing Orion Studio

No public Orion download endpoint or Winget package is assumed to be available.
Build from source using the [Windows development guide](./development/windows.md).

If maintainers publish a signed installer, it will appear on
[GitHub Releases](https://github.com/orion-agents/orion-studio/releases).
Verify its signature, checksum, architecture, and release notes before running
it. A package with a Zed-branded publisher or ID installs the upstream product,
not Orion Studio.

## Uninstall

- Installed via installer: Use `Settings` → `Apps` → `Installed apps`, search for Orion Studio, and click Uninstall.
- Built from source: Remove the build output directory you created (e.g., your target/install folder).

Your settings and extensions live in your user profile. When uninstalling, you can choose to keep or remove them.

## Remote Development (SSH)

Orion Studio supports remote development on Windows through both SSH and WSL. You can connect to remote servers via SSH or work with files inside WSL distributions directly from Orion Studio.

For detailed instructions on setting up and using remote development features, including SSH configuration, WSL setup, and troubleshooting, see the [Remote Development documentation](./remote-development.md).

## Troubleshooting

### Orion Studio fails to start or shows a blank window

- Check that your hardware and operating system version are compatible with Orion Studio. See our [installation guide](./installation.md) for more information.
- Update your GPU drivers from your GPU vendor (Intel/AMD/NVIDIA/Qualcomm).
- Ensure hardware acceleration is enabled in Windows and not blocked by third‑party software.
- Try launching Orion Studio with no extensions or custom settings to isolate conflicts.

### Terminal issues

If activation scripts don’t run, update to the latest version and verify your shell profile files are not exiting early. For Git operations, confirm Git Bash or PowerShell is available and on PATH.

### SSH remoting problems

When prompted for credentials, use the graphical askpass dialog. If it doesn’t appear, check for credential manager conflicts and that GUI prompts aren’t blocked by your terminal.

### Graphics issues

#### Orion Studio fails to open / degraded performance

Orion Studio requires a DirectX 11 compatible GPU to run. If Orion Studio fails to open, your GPU may not meet the minimum requirements.

To check if your GPU supports DirectX 11, run the following command:

```powershell
dxdiag
```

This will open the DirectX Diagnostic Tool, which shows the DirectX version your GPU supports under `System` → `System Information` → `DirectX Version`.

If you're running Orion Studio inside a virtual machine, it will use the emulated adapter provided by your VM. While Orion Studio will work in this environment, performance may be degraded.
