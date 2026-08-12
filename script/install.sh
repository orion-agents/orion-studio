#!/usr/bin/env sh
set -eu

# Downloads a tarball from https://orion.dev/releases and unpacks it
# into ~/.local/. If you'd prefer to do this manually, instructions are at
# https://orion.dev/docs/linux.

main() {
    platform="$(uname -s)"
    arch="$(uname -m)"
    channel="${ORION_STUDIO_CHANNEL:-${ZED_CHANNEL:-stable}}"
    ZED_VERSION="${ORION_STUDIO_VERSION:-${ZED_VERSION:-latest}}"
    ZED_BUNDLE_PATH="${ORION_STUDIO_BUNDLE_PATH:-$ZED_BUNDLE_PATH}"
    # Use TMPDIR if available (for environments with non-standard temp directories)
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        temp="$(mktemp -d "$TMPDIR/zed-XXXXXX")"
    else
        temp="$(mktemp -d "/tmp/zed-XXXXXX")"
    fi

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    case "$platform-$arch" in
        macos-arm64* | linux-arm64* | linux-aarch64)
            arch="aarch64"
            ;;
        macos-x86* | linux-x86*)
            arch="x86_64"
            ;;
        *)
            echo "Unsupported platform or architecture"
            exit 1
            ;;
    esac

    if command -v curl >/dev/null 2>&1; then
        curl () {
            command curl -fL "$@"
        }
    elif command -v wget >/dev/null 2>&1; then
        curl () {
            wget -O- "$@"
        }
    else
        echo "Could not find 'curl' or 'wget' in your path"
        exit 1
    fi

    "$platform" "$@"

    if [ "$(command -v orion-studio)" = "$HOME/.local/bin/orion-studio" ]; then
        echo "Orion Studio has been installed. Run with 'orion-studio'"
    else
        echo "To run Orion Studio from your terminal, you must add ~/.local/bin to your PATH"
        echo "Run:"

        case "$SHELL" in
            *zsh)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.zshrc"
                echo "   source ~/.zshrc"
                ;;
            *fish)
                echo "   fish_add_path -U $HOME/.local/bin"
                ;;
            *)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.bashrc"
                echo "   source ~/.bashrc"
                ;;
        esac

        echo "To run Orion Studio now, '~/.local/bin/orion-studio'"
    fi
}

linux() {
    if [ -n "${ZED_BUNDLE_PATH:-}" ]; then
        cp "$ZED_BUNDLE_PATH" "$temp/orion-studio-linux-$arch.tar.gz"
    else
        echo "Downloading Orion Studio version: $ZED_VERSION"
        curl "https://cloud.orion.dev/releases/$channel/$ZED_VERSION/download?asset=orion-studio&arch=$arch&os=linux&source=install.sh" > "$temp/orion-studio-linux-$arch.tar.gz"
    fi

    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    case "$channel" in
      stable)
        appid="dev.orion.OrionStudio"
        ;;
      nightly)
        appid="dev.orion.OrionStudio-Nightly"
        ;;
      preview)
        appid="dev.orion.OrionStudio-Preview"
        ;;
      dev)
        appid="dev.orion.OrionStudio-Dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.orion.OrionStudio"
        ;;
    esac

    # Unpack
    rm -rf "$HOME/.local/orion-studio$suffix.app"
    mkdir -p "$HOME/.local/orion-studio$suffix.app"
    tar -xzf "$temp/orion-studio-linux-$arch.tar.gz" -C "$HOME/.local/"

    # Setup ~/.local directories
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"

    # Link the binary
    if [ -f "$HOME/.local/orion-studio$suffix.app/bin/orion-studio" ]; then
        ln -sf "$HOME/.local/orion-studio$suffix.app/bin/orion-studio" "$HOME/.local/bin/orion-studio"
    else
        # support for versions before 0.139.x.
        ln -sf "$HOME/.local/orion-studio$suffix.app/bin/cli" "$HOME/.local/bin/orion-studio"
    fi
    # Legacy compatibility alias for muscle memory / existing scripts.
    ln -sf "$HOME/.local/bin/orion-studio" "$HOME/.local/bin/zed"

    # Copy .desktop file
    desktop_file_path="$HOME/.local/share/applications/${appid}.desktop"
    src_dir="$HOME/.local/orion-studio$suffix.app/share/applications"
    if [ -f "$src_dir/${appid}.desktop" ]; then
        cp "$src_dir/${appid}.desktop" "${desktop_file_path}"
    else
        # Fallback for older tarballs
        cp "$src_dir/orion-studio$suffix.desktop" "${desktop_file_path}"
    fi
    sed -i "s|Icon=orion-studio|Icon=$HOME/.local/orion-studio$suffix.app/share/icons/hicolor/512x512/apps/orion-studio.png|g" "${desktop_file_path}"
    sed -i "s|Exec=orion-studio|Exec=$HOME/.local/orion-studio$suffix.app/bin/orion-studio|g" "${desktop_file_path}"
}

macos() {
    echo "Downloading Orion Studio version: $ZED_VERSION"
    curl "https://cloud.orion.dev/releases/$channel/$ZED_VERSION/download?asset=orion-studio&os=macos&arch=$arch&source=install.sh" > "$temp/Orion-Studio-$arch.dmg"
    hdiutil attach -quiet "$temp/Orion-Studio-$arch.dmg" -mountpoint "$temp/mount"
    app="$(cd "$temp/mount/"; echo *.app)"
    echo "Installing $app"
    if [ -d "/Applications/$app" ]; then
        echo "Removing existing $app"
        rm -rf "/Applications/$app"
    fi
    ditto "$temp/mount/$app" "/Applications/$app"
    hdiutil detach -quiet "$temp/mount"

    mkdir -p "$HOME/.local/bin"
    # Link the binary
    ln -sf "/Applications/$app/Contents/MacOS/cli" "$HOME/.local/bin/orion-studio"
    # Legacy compatibility alias.
    ln -sf "$HOME/.local/bin/orion-studio" "$HOME/.local/bin/zed"
}

main "$@"
