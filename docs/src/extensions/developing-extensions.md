---
title: Developing Extensions
description: "Create Orion Studio extensions: languages, themes, debuggers, and more."
---

# Developing Extensions {#developing-extensions}

Orion Studio extensions are Git repositories containing an `extension.toml` manifest. They can provide languages, themes, debuggers, snippets, and MCP servers.

## Extension Features {#extension-features}

Extensions can provide:

- [Languages](./languages.md)
- [Debuggers](./debugger-extensions.md)
- [Themes](./themes.md)
- [Icon Themes](./icon-themes.md)
- [Snippets](./snippets.md)
- [MCP Servers](./mcp-extensions.md)

## Developing an Extension Locally

Before starting to develop an extension for Orion Studio, be sure to [install Rust via rustup](https://www.rust-lang.org/tools/install).

> Orion Studio uses the `wasm32-wasip2` Rust target to compile extensions. If Rust is installed via rustup, Orion Studio will install the target automatically. If Rust is installed another way (e.g., via Homebrew or Nix), you must make the `wasm32-wasip2` target available yourself — for example, by adding it to the `targets` of a Nix rust-overlay or fenix toolchain.

Extensions that provide grammars additionally require the [wasi-sdk](https://github.com/WebAssembly/wasi-sdk) to compile Tree-sitter parsers. Orion Studio can download it from its configured source when network access is available, or you can point Orion Studio at an existing installation by setting the `WASI_SDK_PATH` environment variable to its root directory (the one containing `bin/clang`).

When developing an extension, you can use it in Orion Studio without needing to publish it by installing it as a _dev extension_.

From the extensions page, click the `Install Dev Extension` button (or the {#action zed::InstallDevExtension} action) and select the directory containing your extension.

If you need to troubleshoot, check `Orion Studio.log` with {#action zed::OpenLog}. For debug output, close and relaunch Orion Studio with `orion --foreground`, which shows more verbose INFO-level logs.

If you already have the published version of the extension installed, the published version will be uninstalled prior to the installation of the dev extension. After successful installation, the `Extensions` page will indicate that the upstream extension is "Overridden by dev extension".

## Directory Structure of an Orion Studio Extension {#directory-structure-of-a-zed-extension}

This section keeps its historical `directory-structure-of-a-zed-extension`
anchor for incoming-link compatibility.

An Orion Studio extension is a Git repository that contains an `extension.toml`. This file must contain some
basic information about the extension:

```toml
id = "my-extension"
name = "My extension"
version = "0.0.1"
schema_version = 1
authors = ["Your Name <you@example.com>"]
description = "Example extension"
repository = "https://github.com/your-name/my-orion-extension"
```

In addition to this, several optional files and directories can add functionality to an Orion Studio extension. An example extension with all capabilities has this structure:

```text
my-extension/
  extension.toml
  Cargo.toml
  src/
    lib.rs
  languages/
    my-language/
      config.toml
      highlights.scm
  themes/
    my-theme.json
  snippets/
    snippets.json
    rust.json
```

## Rust and WebAssembly

> Please note that most extensions will work properly without any Rust code present. In particular, only language server, context server and debugger extensions require the presence of custom Rust in order to function properly.

Procedural parts of extensions are written in Rust and compiled to WebAssembly. To develop an extension that includes custom code, include a `Cargo.toml` like this:

```toml
[package]
name = "my-extension"
version = "0.0.1"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
zed_extension_api = "0.1.0"
```

Use the latest compatible version of [`zed_extension_api`](https://crates.io/crates/zed_extension_api). The crate name, Rust alias `zed`, and [`compatible-zed-versions` anchor](https://github.com/orion-agents/orion-studio/blob/main/crates/extension_api#compatible-zed-versions) are retained extension-ABI identifiers from upstream Zed.

In the `src/lib.rs` file in your Rust crate you will need to define a struct for your extension and implement the `Extension` trait, as well as use the `register_extension!` macro to register your extension:

```rs
use zed_extension_api as zed;

struct MyExtension {
    // ... state
}

impl zed::Extension for MyExtension {
    // ...
}

zed::register_extension!(MyExtension);
```

> Since your extension will be compiled to WebAssembly, some Rust features might not work like you would expect them to. For example, `cfg` - directives will not work and `std::env::var` will also not yield the expected results. Instead, use the [`zed_extension_api::current_platform`](https://docs.rs/zed_extension_api/latest/zed_extension_api/fn.current_platform.html) method to get information about the current environment and familiarize yourself with the [`Worktree` struct and its methods](https://docs.rs/zed_extension_api/latest/zed_extension_api/struct.Worktree.html) for reading environment variables and finding binaries in the user's `PATH`.

### Debugging your Rust extension

`stdout`/`stderr` is forwarded directly to the Orion Studio process. In order to see `println!`/`dbg!` output from your extension, you can start Orion Studio in your terminal with a `--foreground` flag.

## Working with the upstream extension registry

Orion Studio does not assume a public Orion extension registry is deployed. The following workflow targets the upstream Zed registry and applies only when your Orion Studio build is configured to consume it.

1. Fork the repo

> **Note:** `zed-industries/extensions` is owned and reviewed by the upstream Zed project. Follow its contribution policy; Orion Studio maintainers do not control that repository or its publishing decisions.

2. Clone the repo to your local machine

```sh
# Substitute the url of your fork here:
# git clone https://github.com/zed-industries/extensions
cd extensions
git submodule init
git submodule update
```

## Extension License Requirements

As of October 1st, 2025, extension repositories must include a license.
The following licenses are accepted:

- [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0)
- [BSD 2-Clause](https://opensource.org/license/bsd-2-clause)
- [BSD 3-Clause](https://opensource.org/license/bsd-3-clause)
- [CC BY 4.0](https://creativecommons.org/licenses/by/4.0)
- [GNU GPLv3](https://www.gnu.org/licenses/gpl-3.0.en.html)
- [GNU LGPLv3](https://www.gnu.org/licenses/lgpl-3.0.en.html)
- [MIT](https://opensource.org/license/mit)
- [Unlicense](https://unlicense.org)
- [zlib](https://opensource.org/license/zlib)

These are upstream Zed registry requirements. They allow that registry to distribute the binary produced from extension code. Without a valid license, a pull request to add or update an extension there will fail its CI.

Your license file should be at the root of your extension repository. Any filename that has `LICENCE` or `LICENSE` as a prefix (case insensitive) will be inspected to ensure it matches one of the accepted licenses. See the [license validation source code](https://github.com/zed-industries/extensions/blob/main/src/lib/license.js).

> This license requirement applies only to your extension code itself (the code that gets compiled into the extension binary).
> It does not apply to any tools your extension may download or interact with, such as language servers or other external dependencies.
> If your repository contains both extension code and other projects (like a language server), you are not required to relicense those other projects — only the extension code needs to be one of the aforementioned accepted licenses.

## Extension Publishing Prerequisites

Before publishing your extension, make sure that you have chosen a unique extension ID for your extension in the [extension manifest](#directory-structure-of-a-zed-extension).
This will be the primary identifier for your extension and cannot be changed after your extension has been published.
Also, ensure that you have filled out all the required fields in the manifest.

Before publishing to the upstream Zed registry, make sure the extension follows that registry's current preconditions:

- Extension IDs and names must not contain `zed`, `Zed`, or `extension`, under the upstream registry's naming policy.
- Your extension ID should provide some information on what your extension tries to accomplish. E.g. for themes, it should be suffixed with `-theme`, snippet extensions should be suffixed with `-snippets` and so on. An exception to that rule is an extension that provides support for languages or popular tooling that people would expect to find under that ID. You can take a look at the list of [existing extensions](https://github.com/zed-industries/extensions/blob/main/extensions.toml) to get a grasp on how this usually is enforced.
- Your extension must only include the resources it requires to function and nothing else.
  - See the [directory structure of an Orion Studio extension](#directory-structure-of-a-zed-extension) and the [Rust and WebAssembly](#rust-and-webassembly) sections for more information. The anchor retains its historical Zed name for incoming-link compatibility.
- Extensions must in no way attempt to read nor modify the environment outside of the environment designated to them by Orion Studio. Should they need to read the environment, they should use methods as provided by the [Orion Studio Rust Extension API](https://docs.rs/zed_extension_api/latest/zed_extension_api/) and may fall back to appropriate methods from the Rust standard library. Should they need changes to the environment, they must instead ask the user to perform these for them using an appropriate method within the context (e.g. provide information for doing so using the `ContextServerConfiguration` for context servers).
  - Please make sure to have read the [Rust and WebAssembly section above](#rust-and-webassembly) for more information and help regarding this topic.
- For submissions to the upstream Zed registry, extensions should provide
  something that is not already available there instead of replacing an
  existing extension that could be fixed. If an existing extension's language
  server support is broken, first try contributing a fix to that extension.
  - If you receive no response within the extension's repository, document those attempts in any upstream registry pull request. Upstream Zed reviewers decide how to proceed.
- Extensions that intend to provide a language, debugger or MCP server must not ship the language server as part of the extension. Instead, the extension should either download the language server or check for the availability of the language server in the user's environment using the APIs as provided by the [Orion Studio Rust Extension API](https://docs.rs/zed_extension_api/latest/zed_extension_api/).
- Themes and icon themes should not be published as part of extensions that provide other features, e.g. language support. Instead, they should be published as a distinct extension. This also applies to themes and icon themes living in the same repository.

Upstream reviewers may delay or reject submissions that do not follow those rules. A separately deployed Orion registry may define different policies.

## Publishing your extension

> Prior to publishing your extension, you should have installed as well as tested it locally thoroughly. Furthermore, you should have read the [prerequisites above](#extension-publishing-prerequisites). Note that untested extension submissions where the extension is not functioning at all will be closed eagerly without further feedback.

To publish to the upstream Zed registry, open a PR to [the `zed-industries/extensions` repository](https://github.com/zed-industries/extensions). The repository and organization names are retained because they identify the actual upstream service.

In your PR, do the following:

1. Add your extension as a Git submodule within the `extensions/` directory under the `extensions/{extension-id}` path

```sh
git submodule add https://github.com/your-username/my-orion-extension.git extensions/my-extension
git add extensions/my-extension
```

> All extension submodules must use HTTPS URLs and not SSH URLS (`git@github.com`). Furthermore, your extension repository must be publicly available and the checked out submodule commit must be on a branch and thus not be a detached commit.

2. Add a new entry to the top-level `extensions.toml` file containing your extension:

```toml
[my-extension]
submodule = "extensions/my-extension"
version = "0.0.1"
```

If your extension is in a subdirectory within the submodule, you can use the `path` field to point to where the extension resides:

```toml
[my-extension]
submodule = "extensions-my-extension"
path = "packages/orion-studio"
version = "0.0.1"
```

> Note that the [required extension license](#extension-license-requirements) must reside at the specified path, a license at the root of the repository will not work. However, you are free to symlink an existing license within the repository or choose an alternative license from the list of accepted licenses for the extension code.

3. Run `pnpm sort-extensions` to ensure `extensions.toml` and `.gitmodules` are sorted

Once the upstream PR is merged, the extension is packaged for the Zed extension registry. It becomes visible in Orion Studio only when the build is configured to consume that registry and the extension remains ABI-compatible.

## Updating an extension

To update an extension in the upstream Zed registry, open a PR to [the `zed-industries/extensions` repository](https://github.com/zed-industries/extensions).

In your PR do the following:

1. Update the extension's submodule to the commit of the new version. For this, you can run

```sh
# From the root of the repository:
git submodule update --remote extensions/your-extension-name
```

to update your extension to the latest commit available in your remote repository.

2. Update the `version` field for the extension in `extensions.toml`
   - Make sure the `version` matches the one set in `extension.toml` at the particular commit.

If you'd like to automate this upstream process, there is a third-party, Zed-named [community GitHub Action](https://github.com/huacnlee/zed-extension-action) you can use.

> **Note:** If your extension repository has a different license, you'll need to update it to be one of the [accepted extension licenses](#extension-license-requirements) before publishing your update.
