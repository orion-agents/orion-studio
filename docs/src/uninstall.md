---
title: Uninstall Orion Studio
description: Remove Orion Studio and, optionally, its user data on macOS, Linux, and Windows.
---

# Uninstall Orion Studio

Quit Orion Studio before removing the application. Removing user data is optional and cannot be undone, so back up settings you want to keep.

## macOS

Remove `Orion Studio.app` from the location where you installed it, normally `/Applications` or `~/Applications`.

To remove Orion Studio user data as well, delete only the paths that exist:

- `~/Library/Application Support/Orion Studio`
- `~/Library/Saved Application State/dev.orion.OrionStudio.savedState`
- `~/Library/Logs/Orion Studio`
- `~/Library/Caches/dev.orion.OrionStudio`
- `~/Library/Caches/Orion Studio`
- `~/.config/orion-studio`
- `~/.local/state/Orion Studio`

No Orion Studio Homebrew cask is assumed to be published. Casks named `zed` install or remove the upstream Zed application, not Orion Studio.

## Linux

If you installed a release bundle with the repository installer, run:

```sh
orion-studio --uninstall
```

If `orion-studio` is not on your `PATH`, invoke the packaged launcher directly:

```sh
$HOME/.local/orion-studio.app/bin/orion-studio --uninstall
```

For a manual installation, remove the paths you created. The repository installer normally uses:

- application bundle: `~/.local/orion-studio.app`
- canonical launcher: `~/.local/bin/orion-studio`
- optional short alias: `~/.local/bin/orion`
- deprecated legacy alias: `~/.local/bin/zed`
- configuration: `~/.config/orion-studio`
- data: `~/.local/share/orion-studio`
- state: `~/.local/state/orion-studio`

If a package manager supplied Orion Studio, use that package manager's uninstall command and package identifier. Packages named `zed` or `zed-editor` refer to upstream Zed unless the package maintainer explicitly states otherwise.

## Windows

If Orion Studio appears in **Settings → Apps → Installed apps**, select it and choose **Uninstall**. For a portable or locally built installation, remove the directory where you placed `orion-studio.exe`.

To remove Orion Studio user data as well, delete only the paths that exist:

- `%APPDATA%\Orion Studio`
- `%LOCALAPPDATA%\Orion Studio`

## Legacy Zed Data

Orion Studio can migrate selected data from legacy Zed locations. Those locations intentionally retain `Zed`, `zed`, or `dev.zed` in their names. Delete them only if you no longer use upstream Zed and no longer need migration or rollback data.

Examples include:

- macOS: `~/Library/Application Support/Zed`, `~/.config/zed`, and `~/Library/Caches/dev.zed.Zed`
- Linux: `~/.config/zed`, `~/.local/share/zed`, and `~/.local/state/zed`
- Windows: `%APPDATA%\Zed` and `%LOCALAPPDATA%\Zed`

For installation-specific Linux troubleshooting, see [Orion Studio on Linux](./linux.md). For other problems, file a [GitHub issue](https://github.com/orion-agents/orion-studio/issues/new/choose).
