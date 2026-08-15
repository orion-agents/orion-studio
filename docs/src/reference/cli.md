---
title: CLI Reference
description: "Reference for Orion Studio's command-line interface (CLI), including opening files and directories, integrating with tools, and controlling Orion Studio from scripts."
---

# CLI Reference

Use Orion Studio's command-line interface (CLI) to open files and directories, integrate with other tools, and control Orion Studio from scripts.

## Installation

**macOS and Linux:** Orion Studio release bundles install `orion-studio` as the
canonical launcher. The same bundles may also install `orion` as an optional
short alias and `zed` as a deprecated compatibility alias. If you build from
source, create the launcher symlink yourself or invoke the application binary
directly.

**Windows:** Add Orion Studio's installation directory to your `PATH`, or use
the full path to `orion-studio.exe`.

The examples below use the canonical `orion-studio` command.

## Usage

```sh
orion-studio [OPTIONS] [PATHS]...
```

## Opening Files and Directories

Open a file:

```sh
orion-studio myfile.txt
```

Open a directory as a workspace:

```sh
orion-studio ~/projects/myproject
```

Open multiple files or directories:

```sh
orion-studio file1.txt file2.txt ~/projects/myproject
```

Open a file at a specific line and column:

```sh
orion-studio myfile.txt:42        # Open at line 42
orion-studio myfile.txt:42:10     # Open at line 42, column 10
```

## Options

### `-w`, `--wait`

Wait for all opened files to be closed before the CLI exits. When opening a directory, waits until the window is closed.

This is useful for integrating Orion Studio with tools that expect an editor to block until editing is complete (e.g., `git commit`):

```sh
export EDITOR="orion-studio --wait"
git commit  # Opens Orion Studio and waits for you to close the commit message file
```

### `-n`, `--new`

Open paths in a new workspace window, even if the paths are already open in an existing window:

```sh
orion-studio -n ~/projects/myproject
```

### `-a`, `--add`

Add paths to the currently focused workspace instead of opening a new window. When multiple workspace windows are open, files open in the focused window:

```sh
orion-studio -a newfile.txt
```

### `-r`, `--reuse`

Reuse an existing window, replacing its current workspace with the new paths:

```sh
orion-studio -r ~/projects/different-project
```

### `-e`, `--existing`

Open paths in an existing Orion Studio window instead of creating a new one:

```sh
orion-studio -e myfile.txt
```

By default (without `-n`, `-a`, `-r`, or `-e`), directories open in the current window's sidebar. You can change this default with the `cli_default_open_behavior` setting. See [Windows & Projects](../windows-and-projects.md) for more details.

### `--diff <OLD_PATH> <NEW_PATH>`

Open a diff view comparing two files. Can be specified multiple times:

```sh
orion-studio --diff file1.txt file2.txt
orion-studio --diff old.rs new.rs --diff old2.rs new2.rs
```

### `--foreground`

Run Orion Studio in the foreground, keeping the terminal attached. Useful for debugging:

```sh
orion-studio --foreground
```

### `--user-data-dir <DIR>`

Use a custom directory for all user data (database, extensions, logs) instead of the default location:

```sh
orion-studio --user-data-dir ~/.orion-studio-custom
```

Default locations:

- **macOS:** `~/Library/Application Support/Orion Studio`
- **Linux:** `$XDG_DATA_HOME/orion-studio` (typically `~/.local/share/orion-studio`)
- **Windows:** `%LOCALAPPDATA%\Orion Studio`

### `-v`, `--version`

Print Orion Studio's version and exit:

```sh
orion-studio --version
```

### `--completions <SHELL>`

Generate shell completions for the `orion-studio` CLI:

#### Bash

Add to `~/.bashrc`:

```bash
eval "$(orion-studio --completions bash)"
```

#### Elvish

Add to `~/.config/elvish/rc.elv`:

```elvish
set edit:completion:arg-completer[orion-studio] = { |@args|
    eval (orion-studio --completions elvish | slurp)
    $edit:completion:arg-completer[orion-studio] $@args
}
```

#### Fish

Add to `~/.config/fish/config.fish`:

```fish
orion-studio --completions fish | source
```

#### Nushell

Add to `~/.config/nushell/config.nu`:

```nu
mkdir ($nu.data-dir | path join "vendor/autoload")
^orion-studio --completions nushell | save --force ($nu.data-dir | path join "vendor/autoload/orion-studio.nu")
```

#### Powershell

Add to `$PROFILE`:

```powershell
(&orion-studio --completions powershell) | Out-String | Invoke-Expression
```

#### Zsh

Add to `~/.zshrc`:

```zsh
eval "$(orion-studio --completions zsh)"
```

### `--uninstall`

Uninstall Orion Studio and remove all related files (macOS and Linux only):

```sh
orion-studio --uninstall
```

### `--orion-studio <PATH>`

Specify a custom path to the Orion Studio application or binary:

```sh
orion-studio --orion-studio "/path/to/Orion Studio.app" myfile.txt
```

`--zed` remains a deprecated alias for upgrade compatibility.

## Reading from Standard Input

Read content from stdin by passing `-` as the path:

```sh
echo "Hello, World!" | orion-studio -
cat myfile.txt | orion-studio -
ps aux | orion-studio -
```

This creates a temporary file with the stdin content and opens it in Orion Studio.

## URL Handling

The CLI can open `orion://`, `file://`, and `ssh://` URLs:

```sh
orion-studio orion://settings
orion-studio file:///Users/whatever/.zshrc
orion-studio ssh://me@example.com/abs/path
orion-studio ssh://me@example.com:/abs/path
orion-studio ssh://me@example.com/~/project
orion-studio ssh://me@example.com:~/project
```

`zed://` links are accepted only as a legacy compatibility format. New links
should use `orion://`.

## Using Orion Studio as Your Default Editor

Set Orion Studio as your default editor for Git and other tools:

```sh
export EDITOR="orion-studio --wait"
export VISUAL="orion-studio --wait"
```

Add these lines to your shell configuration file (e.g., `~/.bashrc`, `~/.zshrc`).

## macOS: Switching Release Channels

On macOS, you can launch a specific release channel by passing the channel name as the first argument:

```sh
orion-studio --stable myfile.txt
orion-studio --preview myfile.txt
orion-studio --nightly myfile.txt
```

## WSL Integration (Windows)

On Windows, the CLI supports opening paths from WSL distributions. This is handled automatically when launching Orion Studio from within WSL.

## Exit Codes

| Code | Meaning                           |
| ---- | --------------------------------- |
| `0`  | Success                           |
| `1`  | Error (details printed to stderr) |

When using `--wait`, the exit code reflects whether the files were saved before closing.
