---
title: Building Orion Studio for Linux
description: "Guide to building Orion Studio on Linux."
---

# Building Orion Studio for Linux

## Repository

Clone the [Orion Studio repository](https://github.com/orion-agents/orion-studio).

## Dependencies

- Install [rustup](https://www.rust-lang.org/tools/install)

- Install the necessary system libraries:

  ```sh
  script/linux
  ```

  If you prefer to install the system libraries manually, you can find the list of required packages in the `script/linux` file.

## Building from source

Once the dependencies are installed, you can build Orion Studio using [Cargo](https://doc.rust-lang.org/cargo/).

For a debug build of the editor:

```sh
cargo run
```

And to run the tests:

```sh
cargo test --workspace
```

The application package is `orion-studio`. The separate `cli` crate is bundled
as the canonical `orion-studio` launcher in release artifacts; `orion` is an
optional short alias. You can run the CLI crate in development with:

```sh
cargo run -p cli
```

## Installing a development build

You can install a local build on your machine with:

```sh
./script/install-linux
```

This builds `orion-studio` and `cli` in release mode, creates a bundle under
`~/.local/orion-studio.app`, installs the canonical launcher at
`~/.local/bin/orion-studio`, adds the optional `orion` short alias and deprecated
`zed` compatibility alias, and installs desktop integration under
`~/.local/share`.

## Wayland & X11

Orion Studio supports both X11 and Wayland. By default, we pick whichever we can find at runtime. If you're on Wayland and want to run in X11 mode, use the environment variable `WAYLAND_DISPLAY=''`.

## Notes for packaging Orion Studio

This section is for distribution maintainers packaging Orion Studio.

### Technical requirements

Orion Studio has two main binaries:

- Build the `cli` package and expose it in `$PATH` as `orion-studio`. Release
  bundles may also expose `orion` as an optional short alias and `zed` only as a
  deprecated compatibility alias.
- Build the `orion-studio` package and place its main binary at a path the launcher can resolve, preferably `$PREFIX/libexec/orion-studio` or `$PREFIX/lib/orion-studio/orion-studio`.
- The source tree still stores the main package under the historical `crates/zed`
  path; the desktop template is `crates/zed/resources/orion-studio.desktop.in`.
  Package the generated desktop file under the Orion application ID and product
  name.
- You will need to ensure that the necessary libraries are installed. You can get the current list by [inspecting the built binary](https://github.com/orion-agents/orion-studio/blob/935cf542aebf55122ce6ed1c91d0fe8711970c82/script/bundle-linux#L65-L67) on your system.
- For an example of a complete build script, see [script/bundle-linux](https://github.com/orion-agents/orion-studio/blob/935cf542aebf55122ce6ed1c91d0fe8711970c82/script/bundle-linux).
- You can disable Orion Studio's auto updates and provide package-specific
  update instructions by building with `ORION_STUDIO_UPDATE_EXPLANATION`, for
  example `ORION_STUDIO_UPDATE_EXPLANATION="Please use Flatpak to update Orion Studio."`.
  The legacy `ZED_UPDATE_EXPLANATION` name remains supported for compatible
  build pipelines.
- Set `crates/zed/RELEASE_CHANNEL` to `nightly`, `preview`, or `stable`, with no trailing newline. The `crates/zed` path is retained for source compatibility.

### Other things to note

Distribution maintainers should account for these behaviors:

- Use `orion-studio` as the packaged launcher. `orion` may be provided as a short
  alias. Do not use `zed` as the primary name; it is deprecated, can collide with
  the upstream editor, and is retained only as a migration alias.
- Orion Studio automatically installs versions of common developer tools, similar to rustup/rbenv/pyenv. This behavior is discussed [here](https://github.com/orion-agents/orion-studio/issues/12589).
- Orion Studio remains compatible with extensions from the upstream [`zed-industries/extensions`](https://github.com/zed-industries/extensions) ecosystem. The organization name is an upstream attribution, not Orion Studio branding. Extensions may install additional tools such as language servers.
- Hosted AI, telemetry, authentication, and collaboration require explicitly configured endpoints. Packages must not silently fall back to Zed-hosted infrastructure. Review the [default settings](https://github.com/orion-agents/orion-studio/blob/main/assets/settings/default.json) and operator configuration before distribution.
- Because of the points above, Orion Studio currently does not work well with sandboxes. See [this discussion](https://github.com/orion-agents/orion-studio/pull/12006#issuecomment-2130421220).

## Flatpak

> Orion Studio's current Flatpak integration exits the sandbox on startup. Workflows that rely on Flatpak's sandboxing may not work as expected.

To build & install the Flatpak package locally follow the steps below:

1. Install Flatpak for your distribution as outlined [here](https://flathub.org/setup).
2. Run the `script/flatpak/deps` script to install the required dependencies.
3. Run `script/flatpak/bundle-flatpak`.
4. Now the package has been installed and has a bundle available at `target/release/{app-id}.flatpak`.

## Memory profiling

[`heaptrack`](https://github.com/KDE/heaptrack) is quite useful for diagnosing memory leaks. To install it:

```sh
$ sudo apt install heaptrack heaptrack-gui
$ cargo install cargo-heaptrack
```

Then, to build and run Orion Studio with the profiler attached:

```sh
$ cargo heaptrack -b orion-studio
```

When this Orion Studio instance exits, terminal output includes a command to run `heaptrack_interpret`, which converts the `*.raw.zst` profile to a `*.zst` file for `heaptrack_gui`.

## Perf recording

How to get a flamegraph with resolved symbols from a running Orion Studio instance.
Use this when Orion Studio is using a lot of CPU. It is not useful for hangs.

### During the incident

- Find the `orion-studio` process ID with `pgrep -x orion-studio`, or locate the process with the highest memory usage in `htop`, `btop`, or `top`.

- Install perf:
  On Ubuntu (derivatives) run `sudo apt install linux-tools`.

- Perf record:
  Run `sudo perf record -p <pid you just found>`, wait a few seconds to gather data, then press Ctrl+C. You should now have a `perf.data` file.

- Make the output file user owned:
  run `sudo chown $USER:$USER perf.data`

- Get build info by opening Orion Studio and running {#action zed::About} from the Command Palette to obtain the exact commit. The `zed::About` action namespace is a retained internal identifier.

Attach `perf.data` and the exact commit to a private maintainer channel or GitHub issue only after checking the profile for sensitive data.

### Later

This can be done by a maintainer with access to the matching source revision.

- Build Orion Studio with symbols:
  Check out the commit found previously and modify `Cargo.toml`.
  Apply the following diff, then make a release build.

```diff
[profile.release]
-debug = "limited"
+debug = "full"
```

- Add the symbols to the perf database:
  `perf buildid-cache -v -a <path to release orion-studio binary>`

- Resolve the symbols from the db:
  `perf inject -i perf.data -o perf_with_symbols.data`

- Install flamegraph:
  `cargo install cargo-flamegraph`

- Render the flamegraph:
  `flamegraph --perfdata perf_with_symbols.data`

## Troubleshooting

### Cargo errors claiming that a dependency is using unstable features

Try `cargo clean` and `cargo build`.
