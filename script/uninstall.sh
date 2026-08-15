#!/usr/bin/env sh
set -eu

# Uninstalls Orion Studio that was installed using the install.sh script

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
            rm -rf "$HOME/.config/zed"
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
        appid="dev.orion.OrionStudio"
        db_suffix="stable"
        ;;
      nightly)
        appid="dev.orion.OrionStudio-Nightly"
        db_suffix="nightly"
        ;;
      preview)
        appid="dev.orion.OrionStudio-Preview"
        db_suffix="preview"
        ;;
      dev)
        appid="dev.orion.OrionStudio-Dev"
        db_suffix="dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="dev.orion.OrionStudio"
        db_suffix="stable"
        ;;
    esac

    # Remove the app directory
    rm -rf "$HOME/.local/orion-studio$suffix.app"

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/orion-studio"
    rm -f "$HOME/.local/bin/zed"

    # Remove the .desktop file
    rm -f "$HOME/.local/share/applications/${appid}.desktop"

    # Remove the database directory for this channel
    rm -rf "$HOME/.local/share/orion-studio/db/0-$db_suffix"
    rm -rf "$HOME/.local/share/zed/db/0-$db_suffix"

    # Remove socket file
    rm -f "$HOME/.local/share/orion-studio/orion-studio-$db_suffix.sock"
    rm -f "$HOME/.local/share/zed/zed-$db_suffix.sock"

    # Remove the entire Orion Studio directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/.local/share/orion-studio"
        rm -rf "$HOME/.local/share/zed"
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

    # Remove the app bundle
    if [ -d "/Applications/$app" ]; then
        rm -rf "/Applications/$app"
    fi

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/orion-studio"
    rm -f "$HOME/.local/bin/zed"

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
