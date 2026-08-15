---
title: Orion Studio on macOS
description: "Orion Studio is developed primarily on macOS, making it a first-class platform with full feature support."
---

# Orion Studio on macOS

Orion Studio is developed primarily on macOS, making it a first-class platform with full feature support.

## Installing Orion Studio

No public Orion download or Homebrew cask is assumed to be available. Build
from source using the guide below. If maintainers publish a signed `.dmg`, it
will appear on [GitHub Releases](https://github.com/orion-agents/orion-studio/releases).
Verify its signature, checksum, architecture, and release notes before dragging
Orion Studio into Applications.

Do not install a Zed-branded cask when you intend to install Orion Studio; that
cask belongs to the upstream product.

### Building from Source

To build Orion Studio from source, see the [macOS development documentation](./development/macos.md).

## System Requirements

- macOS 10.15.7 (Catalina) or later
- Apple Silicon (M1/M2/M3/M4) or Intel processor

Orion Studio uses Metal for GPU-accelerated rendering, which is available on all supported macOS versions.

## Installing the CLI

The app bundle includes a command-line launcher. The in-app **Install CLI**
action creates `/usr/local/bin/orion-studio`; release installation scripts use
`~/.local/bin/orion-studio`. For a source or manually copied build, create the
canonical symlink yourself and optionally add the short alias:

```sh
mkdir -p ~/.local/bin
ln -sf "/Applications/Orion Studio.app/Contents/MacOS/cli" ~/.local/bin/orion-studio
ln -sf ~/.local/bin/orion-studio ~/.local/bin/orion
```

`orion` is only a short alias. `zed` is a deprecated migration alias and must
not replace a launcher owned by an upstream Zed installation.

Then open files and folders with the canonical command:

```sh
orion-studio .                    # Open current folder
orion-studio file.txt             # Open a file
orion-studio project/ file.txt    # Open a folder and a file
```

See the [CLI Reference](./reference/cli.md) for all available options.

## Uninstall

1. Quit Orion Studio if it's running
2. Drag Orion Studio from Applications to the Trash
3. Optionally, remove your settings and extensions:

```sh
rm -rf ~/.config/orion-studio
rm -rf ~/Library/Application\ Support/Orion Studio
rm -rf ~/Library/Caches/Orion Studio
rm -rf ~/Library/Logs/Orion Studio
rm -rf ~/Library/Saved\ Application\ State/dev.orion.OrionStudio.savedState
```

If you installed the CLI, remove it with:

```sh
rm -f ~/.local/bin/orion-studio ~/.local/bin/orion /usr/local/bin/orion-studio
```

## Troubleshooting

### Orion Studio won't open or shows "damaged" warning

If macOS reports that Orion Studio is damaged or can't be opened, it's likely a Gatekeeper issue. Try:

1. Right-click (or Control-click) on Orion Studio in Applications
2. Select "Open" from the context menu
3. Click "Open" in the dialog that appears

This tells macOS to trust the application.

If that doesn't work, remove the quarantine attribute:

```sh
xattr -cr /Applications/Orion Studio.app
```

### CLI command not found

If the `orion-studio` command is not available after installation:

1. Check that `~/.local/bin` is in your `PATH`.
2. Verify that the symlink points to the `cli` binary inside the app bundle.
3. Open a new terminal window to reload your shell configuration.

### Can't install CLI {#cant-install-cli}

If the in-app action cannot write `/usr/local/bin/orion-studio`, use a shell
alias or symlink for the canonical command. The bundled `cli` path depends on
where Orion Studio is installed:

```sh
# Default install (Orion Studio in /Applications)
alias orion-studio="/Applications/Orion Studio.app/Contents/MacOS/cli"

# User install (Orion Studio in ~/Applications)
alias orion-studio="$HOME/Applications/Orion Studio.app/Contents/MacOS/cli"

# Preview build (Orion Studio Preview in ~/Applications)
alias orion-studio="$HOME/Applications/Orion Studio Preview.app/Contents/MacOS/cli"
```

Add the line that matches your install to your shell configuration file. Use `~/.zshrc` for Zsh (the default on modern macOS) or `~/.bashrc` for Bash.

After restarting your shell, use `orion-studio` from the terminal:

```sh
orion-studio .              # Open current folder
orion-studio file.txt       # Open a file
```

### GPU or rendering issues

Orion Studio uses Metal for rendering. If you experience graphical glitches:

1. Ensure macOS is up to date
2. Restart your Mac to reset the GPU state
3. Check Activity Monitor for GPU pressure from other apps

### High memory or CPU usage

If Orion Studio uses more resources than expected:

1. Check for runaway language servers in the terminal output ({#action zed::OpenLog})
2. Try disabling extensions one by one to identify conflicts
3. For large projects, consider using [project settings](./reference/all-settings.md#file-scan-exclusions) to exclude unnecessary folders from indexing

For additional help, see the [Troubleshooting guide](./troubleshooting.md) or
[open a GitHub issue](https://github.com/orion-agents/orion-studio/issues/new/choose).
