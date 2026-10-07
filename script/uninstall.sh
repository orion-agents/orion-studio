#!/usr/bin/env sh
set -eu

# Uninstalls Orion Studio that was installed using the install.sh script

path_exists() {
    [ -e "$1" ] || [ -L "$1" ]
}

canonical_launcher_is_owned() {
    [ -L "$canonical_launcher_path" ] || return 1

    canonical_launcher_target=$(readlink "$canonical_launcher_path") || return 1
    [ "$canonical_launcher_target" = "$expected_cli_path" ] || return 1
    [ -x "$expected_cli_path" ]
}

remove_compat_launcher_if_owned() {
    compat_launcher_path="$1"
    compat_launcher_name="$2"

    if ! path_exists "$compat_launcher_path"; then
        return 0
    fi

    if [ -L "$compat_launcher_path" ]; then
        compat_launcher_target=$(readlink "$compat_launcher_path") || compat_launcher_target=""
        if [ "$compat_launcher_target" = "$canonical_launcher_path" ] && canonical_launcher_is_owned; then
            rm -f "$compat_launcher_path"
            return 0
        fi
    fi

    echo "Preserving $compat_launcher_name because Orion Studio ownership could not be verified: $compat_launcher_path" >&2
}

remove_canonical_launcher_if_owned() {
    if ! path_exists "$canonical_launcher_path"; then
        return 0
    fi

    if canonical_launcher_is_owned; then
        rm -f "$canonical_launcher_path"
    else
        echo "Preserving Orion Studio launcher because ownership could not be verified: $canonical_launcher_path" >&2
    fi
}

check_remaining_installations() {
    platform="$(uname -s)"
    if [ "$platform" = "Darwin" ]; then
        # Check for any Orion Studio variants in /Applications
        remaining=$(ls -d "/Applications/Orion Studio"*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    else
        # Check for any Orion Studio variants in ~/.local
        remaining=$(ls -d "$HOME/.local/orion-studio"*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    fi
}

prompt_remove_preferences() {
    printf "Do you want to keep your Orion Studio preferences? [Y/n] "
    read -r response
    case "$response" in
        [nN]|[nN][oO])
            rm -rf "$HOME/.config/orion-studio"
            echo "Preferences removed."
            ;;
        *)
            echo "Preferences kept."
            ;;
    esac
}

main() {
    platform="$(uname -s)"
    channel="${ORION_STUDIO_CHANNEL:-${ZED_CHANNEL:-stable}}"

    case "${HOME:-}" in
        /*)
            ;;
        *)
            echo "HOME must identify an absolute non-root user directory." >&2
            exit 1
            ;;
    esac
    if ! canonical_home=$(cd "$HOME" 2>/dev/null && pwd -P); then
        echo "HOME must identify an existing user directory." >&2
        exit 1
    fi
    if [ "$canonical_home" = "/" ]; then
        echo "HOME must identify a non-root user directory." >&2
        exit 1
    fi
    HOME="$canonical_home"
    export HOME

    case "$channel" in
        stable|nightly|preview|dev)
            ;;
        *)
            echo "Unsupported Orion Studio release channel: $channel" >&2
            exit 1
            ;;
    esac

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    "$platform"

    echo "Orion Studio has been uninstalled"
}

linux() {
    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    db_suffix="stable"
    case "$channel" in
      stable)
        appid="com.orion.OrionStudio"
        db_suffix="stable"
        ;;
      nightly)
        appid="com.orion.OrionStudio.Nightly"
        db_suffix="nightly"
        ;;
      preview)
        appid="com.orion.OrionStudio.Preview"
        db_suffix="preview"
        ;;
      dev)
        appid="com.orion.OrionStudio.Dev"
        db_suffix="dev"
        ;;
    esac

    expected_cli_path="$HOME/.local/orion-studio$suffix.app/bin/orion-studio"
    canonical_launcher_path="$HOME/.local/bin/orion-studio"

    remove_compat_launcher_if_owned "$HOME/.local/bin/zed" "legacy compatibility launcher"
    remove_compat_launcher_if_owned "$HOME/.local/bin/orion" "Orion short launcher"
    remove_canonical_launcher_if_owned

    # Remove the app directory
    rm -rf "$HOME/.local/orion-studio$suffix.app"

    # Remove the .desktop file
    rm -f "$HOME/.local/share/applications/${appid}.desktop"

    # Remove the database directory for this channel
    rm -rf "$HOME/.local/share/orion-studio/db/0-$db_suffix"

    # Remove socket file
    rm -f "$HOME/.local/share/orion-studio/orion-studio-$db_suffix.sock"

    # Remove the entire Orion Studio directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/.local/share/orion-studio"
        prompt_remove_preferences
    fi

    rm -rf "$HOME/.orion_studio_server"
}

macos() {
    app="Orion Studio.app"
    db_suffix="stable"
    app_id="dev.orion.OrionStudio"
    case "$channel" in
      nightly)
        app="Orion Studio Nightly.app"
        db_suffix="nightly"
        app_id="dev.orion.OrionStudio-Nightly"
        ;;
      preview)
        app="Orion Studio Preview.app"
        db_suffix="preview"
        app_id="dev.orion.OrionStudio-Preview"
        ;;
      dev)
        app="Orion Studio Dev.app"
        db_suffix="dev"
        app_id="dev.orion.OrionStudio-Dev"
        ;;
    esac

    expected_cli_path="/Applications/$app/Contents/MacOS/cli"
    canonical_launcher_path="$HOME/.local/bin/orion-studio"

    remove_compat_launcher_if_owned "$HOME/.local/bin/zed" "legacy compatibility launcher"
    remove_compat_launcher_if_owned "$HOME/.local/bin/orion" "Orion short launcher"
    remove_canonical_launcher_if_owned

    # Remove the app bundle
    if [ -d "/Applications/$app" ]; then
        rm -rf "/Applications/$app"
    fi
    # Remove the database directory for this channel
    rm -rf "$HOME/Library/Application Support/Orion Studio/db/0-$db_suffix"

    # Remove app-specific files and directories
    rm -rf "$HOME/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.ApplicationRecentDocuments/$app_id.sfl"*
    rm -rf "$HOME/Library/Caches/$app_id"
    rm -rf "$HOME/Library/HTTPStorages/$app_id"
    rm -rf "$HOME/Library/Preferences/$app_id.plist"
    rm -rf "$HOME/Library/Saved Application State/$app_id.savedState"

    # Remove Orion Studio data after the final installation is removed.
    if check_remaining_installations; then
        rm -rf "$HOME/Library/Application Support/Orion Studio"
        rm -rf "$HOME/Library/Logs/Orion Studio"

        prompt_remove_preferences
    fi

    rm -rf "$HOME/.orion_studio_server"
}

main "$@"
