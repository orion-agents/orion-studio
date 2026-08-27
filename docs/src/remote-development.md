---
title: Remote Development in Orion Studio - SSH Workflows
description: Use remote development in Orion Studio to edit code over SSH with local UI performance, remote terminals, language servers, and tasks.
---

# Remote Development

Remote Development lets you edit code on a remote server while running Orion Studio locally. The UI stays responsive because it runs on your machine, while language servers, tasks, and terminals run on the server.

For day-to-day workflows, pair remote development with [Tasks](./tasks.md),
[Terminal](./terminal.md), and [Debugger](./debugger.md).

## Overview

Remote development requires two computers: a local machine running the Orion
Studio UI and a remote machine running an Orion Studio headless server. They
communicate over SSH.

On your local machine, Orion Studio runs its UI, talks to language models, uses Tree-sitter to parse and syntax-highlight code, and stores unsaved changes and recent projects. The source code, language servers, tasks, and the terminal all run on the remote server. [AI features](./ai/overview.md) work in remote sessions, including the Agent Panel and Inline Assistant.

> **Upstream history:** older Zed releases offered a hosted remoting mode. Orion
> Studio documents only the current direct SSH transport and does not depend on
> a Zed-hosted remoting service.

## Setup

1. [Build Orion Studio](./installation.md) or install a verified published release.
1. Use {#kb projects::OpenRemote} to open the "Remote Projects" dialog.
1. Click "Connect New Server" and enter the command you use to SSH into the server. See [Supported SSH options](#supported-ssh-options) for options you can pass.
1. Your local machine will attempt to connect using the `ssh` binary on your path. Automatic remote-server download requires a configured Orion release service. Preview and Dev builds never contact that service: they reuse only an exact matching cached server binary or require you to build and install the server manually as described below.
1. Once the Orion Studio server is running, you will be prompted to choose a path to open on the remote server.
   > **Note:** Orion Studio does not currently handle opening very large directories (for example, `/` or `~` that may have >100,000 files) very well. We are working on improving this, but suggest in the meantime opening only specific projects, or subfolders of very large mono-repos.

For simple cases where you don't need any SSH arguments, run `orion ssh://[<user>@]<host>[:<port>]/<path>`. The CLI also accepts `orion ssh://[<user>@]<host>:~/project` and `orion ssh://[<user>@]<host>:/absolute/path`. New deep links use `orion://ssh/[<user>@]<host>[:<port>]/<path>`.

## Supported platforms

The remote machine must be able to run Orion Studio's server. The following platforms should work, though note that we have not exhaustively tested every Linux distribution:

- macOS Catalina or later (Intel or Apple Silicon)
- Linux (x86_64 or arm64, we do not yet support 32-bit platforms)
- Windows (x86_64 or arm64)

## Configuration

The list of remote servers is stored in your settings file {#kb zed::OpenSettings}. You can edit this list using the Remote Projects dialog {#kb projects::OpenRemote}, which provides some robustness - for example it checks that the connection can be established before writing it to the settings file.

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "projects": [{ "paths": ["~/code/orion-studio"] }]
    }
  ]
}
```

Orion Studio shells out to the `ssh` on your path, and so it will inherit any configuration you have in `~/.ssh/config` for the given host. That said, if you need to override anything you can configure the following additional options on each connection:

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "projects": [{ "paths": ["~/code/orion-studio"] }],
      // any argument to pass to the ssh master process
      "args": ["-i", "~/.ssh/work_id_file"],
      "port": 22, // defaults to 22
      // defaults to your username on your local machine
      "username": "me"
    }
  ]
}
```

There are two additional Orion Studio-specific options per connection, `upload_binary_over_ssh` and `nickname`:

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "projects": [{ "paths": ["~/code/orion-studio"] }],
      // When a release service is configured, Orion Studio can download the server remotely.
      // When this is true, it'll be downloaded to your laptop and uploaded over SSH.
      // This is useful when your remote server has restricted internet access.
      "upload_binary_over_ssh": true,
      // Shown in the Orion Studio UI to help distinguish multiple hosts.
      "nickname": "lil-linux"
    }
  ]
}
```

If you open a connection with `orion ssh://192.168.1.10/~/.vimrc`, extra options are read from the first settings entry that matches the host, username, and port.

Although you can pass a password on the command line as `orion ssh://user:password@host/~`, Orion Studio does not write passwords to the settings file. Prefer key-based authentication for repeated connections.

## Remote Development on Windows (SSH)

Orion Studio on Windows supports SSH remoting and will prompt for credentials when needed.

If you encounter authentication issues, confirm that your SSH key agent is running (e.g., ssh-agent or your Git client's agent) and that ssh.exe is on PATH.

### Troubleshooting SSH on Windows

When prompted for credentials, use the graphical askpass dialog. If it doesn't appear, check for credential manager conflicts and that GUI prompts aren't blocked by your terminal.

## WSL Support

Orion Studio supports opening folders inside of WSL natively on Windows.

### Opening a local folder in WSL

To open a local folder inside a WSL container, use the {#action projects::OpenFolderInWsl} action and select the folder you want to open. You will be presented with a list of available WSL distributions to open the folder in.

### Opening a folder already in WSL

To open a folder that's already located inside of a WSL container, use the {#action projects::OpenWsl} action and select the WSL distribution. The distribution will be added to the `Remote Projects` window where you will be able to open the folder.

## Port forwarding

If you'd like to be able to connect to ports on your remote server from your local machine, you can configure port forwarding in your settings file. This is particularly useful for developing websites so you can load the site in your browser while working.

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "port_forwards": [{ "local_port": 8080, "remote_port": 80 }]
    }
  ]
}
```

This will cause requests from your local machine to `localhost:8080` to be forwarded to the remote machine's port 80. Under the hood this uses the `-L` argument to ssh.

By default these ports are bound to localhost, so other computers in the same network as your development machine cannot access them. You can set the local_host to bind to a different interface, for example, 0.0.0.0 will bind to all local interfaces.

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "port_forwards": [
        {
          "local_port": 8080,
          "remote_port": 80,
          "local_host": "0.0.0.0"
        }
      ]
    }
  ]
}
```

These ports also default to the `localhost` interface on the remote host. If you need to change this, you can also set the remote host:

```json [settings]
{
  "ssh_connections": [
    {
      "host": "192.168.1.10",
      "port_forwards": [
        {
          "local_port": 8080,
          "remote_port": 80,
          "remote_host": "docker-host"
        }
      ]
    }
  ]
}
```

## Orion Studio settings

When opening a remote project there are three relevant settings locations:

- The local Orion Studio settings in `~/.config/orion-studio/settings.json` on
  macOS, or `$XDG_CONFIG_HOME/orion-studio/settings.json` on Linux.
- The server Orion Studio settings (in the same place) on the remote server.
- Project settings in `.orion/settings.json` or `.editorconfig`.

Both the local Orion Studio and the server Orion Studio read the project settings, but they are not aware of the other's main settings file.

Which settings file you should use depends on the kind of setting you want to make:

- Project settings should be used for things that affect the project: indentation settings, which formatter / language server to use, etc.
- Server settings should be used for things that affect the server: paths to language servers, proxy settings, etc.
- Local settings should be used for things that affect the UI: font size, etc.

In addition any extensions you have installed locally will be propagated to the remote server. This means that language servers, etc. will run correctly.

## Proxy Configuration

The remote server will not use your local machine's proxy configuration because they may be under different network policies. If your remote server requires a proxy to access the internet, you must configure it on the remote server itself.

In most cases, your remote server will already have proxy environment variables configured. Orion Studio will automatically use them when downloading language servers, communicating with LLM models, etc.

If needed, you can set these environment variables in the server's shell configuration (e.g., `~/.bashrc`):

```bash
export http_proxy="http://proxy.example.com:8080"
export https_proxy="http://proxy.example.com:8080"
export no_proxy="localhost,127.0.0.1"
```

Alternatively, configure the proxy in the remote machine's
`$XDG_CONFIG_HOME/orion-studio/settings.json` on Linux or
`~/.config/orion-studio/settings.json` on macOS:

```json
{
  "proxy": "http://proxy.example.com:8080"
}
```

See the [proxy documentation](./reference/all-settings.md#network-proxy) for supported proxy types and additional configuration options.

## Initializing the remote server

Once you provide the SSH options, Orion Studio shells out to `ssh` on your local machine to create a ControlMaster connection with the options you provide.

Any prompts that SSH needs will be shown in the UI, so you can verify host keys, type key passwords, etc.

Once the master connection is established, Orion Studio will check to see if the remote server binary is present in `~/.orion_server` on the remote, and that its version matches the current version of Orion Studio that you're using.

If the binary is missing or its version does not match, a separately configured
Stable or Nightly deployment can use its release service to download it.
Preview and Dev builds fail closed before hosted artifact lookup and can reuse
only an exact matching artifact already present in the local download cache.
Otherwise install the matching server binary manually. `upload_binary_over_ssh`
changes where an allowed download occurs; it does not enable hosted downloads
for Preview or Dev.

Build the server from this repository:

```bash
cargo build --release --package remote_server
llvm-objcopy --strip-debug target/release/remote_server
```

Stripping debug symbols matches the packaging step in [`script/bundle-linux`](https://github.com/orion-agents/orion-studio/blob/main/script/bundle-linux) and keeps the binary closer to the size of an Orion Studio release bundle. Prefer `llvm-objcopy` because GNU `objcopy` may not understand newer LLVM-emitted CREL sections.

The Linux packaging script builds a musl/static remote server. If you need that,
follow the current target and flags in [`script/bundle-linux`](https://github.com/orion-agents/orion-studio/blob/main/script/bundle-linux); for x86-64 Linux:

```bash
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --package remote_server --target x86_64-unknown-linux-musl
llvm-objcopy --strip-debug target/x86_64-unknown-linux-musl/release/remote_server
```

If you do this, you must upload it to `~/.orion_server/orion-studio-remote-server-{RELEASE_CHANNEL}-{VERSION}` on the server, for example `~/.orion_server/orion-studio-remote-server-stable-0.217.3+stable.105.80433cb239e868271457ac376673a5f75bc4adb1`. The version must exactly match the version of Orion Studio itself you are using.

## Maintaining the SSH connection

Once the server is initialized, Orion Studio will create new SSH connections (reusing the existing ControlMaster) to run the remote development server.

Each connection tries to run the development server in proxy mode. This mode will start the daemon if it is not running, and reconnect to it if it is. This way when your connection drops and is restarted, you can continue to work without interruption.

In the case that reconnecting fails, the daemon will not be re-used. That said, unsaved changes are by default persisted locally, so that you do not lose work. You can always reconnect to the project at a later date and Orion Studio will restore unsaved changes.

For connection failures, inspect the Orion Studio log with `cmd-shift-p Open Log`. If the failure is reproducible, [file a GitHub issue](https://github.com/orion-agents/orion-studio/issues/new).

## Supported SSH Options

Under the hood, Orion Studio shells out to the `ssh` binary to connect to the remote server. We create one SSH control master per project, and then use that to multiplex SSH connections for the Orion Studio protocol itself, any terminals you open and tasks you run. We read settings from your SSH config file, but if you want to specify additional options to the SSH control master you can configure Orion Studio to set them.

When typing in the "Connect New Server" dialog, you can use bash-style quoting to pass options containing a space. Once you have created a server it will be added to the `"ssh_connections": []` array in your settings file. You can edit the settings file directly to make changes to SSH connections.

Supported options:

- `-p` / `-l` - these are equivalent to passing the port and the username in the host string.
- `-L` / `-R` for port forwarding
- `-i` - to use a specific key file
- `-o` - to set custom options
- `-J` / `-w` - to proxy the SSH connection
- `-F` for specifying an `ssh_config`
- And also... `-4`, `-6`, `-A`, `-B`, `-C`, `-D`, `-I`, `-K`, `-P`, `-X`, `-Y`, `-a`, `-b`, `-c`, `-i`, `-k`, `-l`, `-m`, `-o`, `-p`, `-w`, `-x`, `-y`

Note that we deliberately disallow some options (for example `-t` or `-T`) that Orion Studio will set for you.

## Known Limitations

- You can't open files from the remote Terminal by typing the `orion` command.

## See also

- [Running & Testing](./running-testing.md): Run tasks, terminal commands, and
  debugger sessions while you work remotely.
- [Git Worktrees](./git.md#git-worktrees): Create and switch between linked
  Git worktrees. Orion Studio supports the worktree picker in remote projects when the
  remote connection is active.
- [Configuring Orion Studio](./configuring-zed.md): Manage shared and project
  settings, including the canonical `.orion/settings.json` project path.
- [Agent Panel](./ai/agent-panel.md): Use AI workflows in remote projects.
