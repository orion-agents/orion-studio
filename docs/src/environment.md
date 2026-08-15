---
title: Environment Variables - Orion Studio
description: How Orion Studio detects and uses environment variables. Shell integration, dotenv support, and troubleshooting.
---

# Environment Variables

_**Note**: The following only applies to Orion Studio 0.152.0 and later._

Multiple features in Orion Studio are affected by environment variables:

- [Tasks](./tasks.md)
- [Built-in terminal](./terminal.md)
- Look-up of language servers
- Language servers

To make the best use of these features, it helps to understand where Orion Studio gets environment variables and how it uses them.

## Where does Orion Studio get its environment variables from?

How Orion Studio starts affects which environment variables it can use. That includes launching from the macOS Dock, a Linux window manager, or the `orion` CLI.

### Launched from the CLI

If Orion Studio is opened via the CLI (`orion`), it inherits the environment variables from the surrounding shell session.

That means if you do

```sh
$ export MY_ENV_VAR=hello
$ orion .
```

the environment variable `MY_ENV_VAR` is now available inside Orion Studio. For example, in the built-in terminal.

The `orion` CLI passes its environment to Orion Studio even when another Orion Studio instance is already running. The deprecated `zed` launcher alias, if present in a release bundle, follows the same compatibility behavior.

### Launched via window manager, Dock, or launcher

When Orion Studio has been launched via the macOS Dock, or a GNOME or KDE icon on Linux, or an application launcher like Alfred or Raycast, it has no surrounding shell environment from which to inherit its environment variables.

To still have a useful environment, Orion Studio spawns a login shell in the user's home directory and reads its environment. This environment is then set on the Orion Studio _process_, so all Orion Studio windows and projects inherit it.

Since that can lead to problems for users who need different environment variables per project (for example with `direnv`, `asdf`, or `mise`), Orion Studio spawns another login shell when opening a project. This second shell runs in the project's directory. The environment from that shell is _not_ set on the process, because opening a new project would otherwise change the environment for all Orion Studio windows. Instead, that environment is stored and passed along when running tasks, opening terminals, or spawning language servers.

## Where and how are environment variables used?

There are two sets of environment variables:

1. Environment variables of the Orion Studio process
2. Environment variables stored per project

The variables from (1) are always used, since they are stored on the process itself and every spawned process (tasks, terminals, language servers, ...) will inherit them by default.

The variables from (2) are used explicitly, depending on the feature.

### Tasks

Tasks are spawned with a combined environment. In order of precedence (low to high, with the last overwriting the first):

- the Orion Studio process environment
- if the project was opened from the CLI: the CLI environment
- if the project was not opened from the CLI: the project environment variables obtained by running a login shell in the project's root folder
- optional, explicitly configured environment in settings

### Built-in terminal

Built-in terminals, like tasks, are spawned with a combined environment. In order of precedence (low to high):

- the Orion Studio process environment
- if the project was opened from the CLI: the CLI environment
- if the project was not opened from the CLI: the project environment variables obtained by running a login shell in the project's root folder
- optional, explicitly configured environment in settings

### Look-up of language servers

For some languages the language server adapters lookup the binary in the user's `$PATH`. Examples:

- Go
- Zig
- Rust (if [configured to do so](./languages/rust.md#binary))
- C
- TypeScript

For this look-up, Orion Studio uses the following environment:

- if the project was opened from the CLI: the CLI environment
- if the project was not opened from the CLI: the project environment variables obtained by running a login shell in the project's root folder

### Language servers

After looking up a language server, Orion Studio starts it.

These language server processes always inherit Orion Studio's process environment. But, depending on the language server look-up, additional environment variables might be set or overwrite the process environment.

- If the language server was found in the project environment's `$PATH`, then that project environment is passed along to the language server process. Where the project environment comes from depends on how the project was opened (via CLI or not). See the previous section on language server look-up.
- If the language server was not found in the project environment, Orion Studio tries to install and start it globally. In that case, the process inherits Orion Studio's process environment and, if the project was opened via CLI, the CLI environment.
