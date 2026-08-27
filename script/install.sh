#!/usr/bin/env sh
set -eu

# Installs a verified Orion Studio release. Manual installation instructions are
# available at https://orion.dev/docs/linux.

path_exists() {
    [ -e "$1" ] || [ -L "$1" ]
}

remove_path() {
    case "$1" in
        "" | /)
            echo "Refusing to remove an unsafe installer path: '$1'" >&2
            return 1
            ;;
    esac
    rm -rf "$1"
}

restore_auxiliary_path() {
    restore_target="$1"
    restore_backup="$2"
    restore_prepared_marker="$3"
    restore_original_marker="$4"

    if [ -z "$restore_prepared_marker" ] || [ ! -f "$restore_prepared_marker" ]; then
        return 0
    fi

    if [ -f "$restore_original_marker" ]; then
        if path_exists "$restore_backup"; then
            if path_exists "$restore_target"; then
                remove_path "$restore_target" || return 1
            fi
            mv "$restore_backup" "$restore_target" || return 1
        elif ! path_exists "$restore_target"; then
            echo "Could not restore missing installer path: $restore_target" >&2
            return 1
        fi
    elif path_exists "$restore_target"; then
        remove_path "$restore_target" || return 1
    fi
}

cleanup() {
    original_status=$?
    cleanup_failed=0
    trap - EXIT HUP INT TERM
    set +e

    if [ "${activation_state:-idle}" != "committed" ]; then
        restore_auxiliary_path \
            "${desktop_file_path:-}" \
            "${desktop_file_backup:-}" \
            "${desktop_file_prepared_marker:-}" \
            "${desktop_file_original_marker:-}" || cleanup_failed=1
        restore_auxiliary_path \
            "${legacy_cli_link_path:-}" \
            "${legacy_cli_link_backup:-}" \
            "${legacy_cli_link_prepared_marker:-}" \
            "${legacy_cli_link_original_marker:-}" || cleanup_failed=1
        restore_auxiliary_path \
            "${short_cli_link_path:-}" \
            "${short_cli_link_backup:-}" \
            "${short_cli_link_prepared_marker:-}" \
            "${short_cli_link_original_marker:-}" || cleanup_failed=1
        restore_auxiliary_path \
            "${canonical_cli_link_path:-}" \
            "${canonical_cli_link_backup:-}" \
            "${canonical_cli_link_prepared_marker:-}" \
            "${canonical_cli_link_original_marker:-}" || cleanup_failed=1
    fi

    case "${activation_state:-idle}" in
        committed)
            if [ -n "${activation_pending_path:-}" ] && path_exists "$activation_pending_path"; then
                remove_path "$activation_pending_path" || cleanup_failed=1
            fi
            if [ -n "${activation_backup_path:-}" ] && path_exists "$activation_backup_path"; then
                remove_path "$activation_backup_path" || cleanup_failed=1
            fi
            ;;
        idle)
            ;;
        *)
            if [ "${activation_target_had_original:-0}" = "1" ]; then
                if [ -n "${activation_backup_path:-}" ] && path_exists "$activation_backup_path"; then
                    if [ -n "${activation_target_path:-}" ] && path_exists "$activation_target_path"; then
                        remove_path "$activation_target_path" || cleanup_failed=1
                    fi
                    if ! mv "$activation_backup_path" "$activation_target_path"; then
                        echo "Could not restore the previous Orion Studio installation." >&2
                        cleanup_failed=1
                    fi
                elif [ -n "${activation_target_path:-}" ] && ! path_exists "$activation_target_path"; then
                    echo "The previous Orion Studio installation could not be recovered." >&2
                    cleanup_failed=1
                fi
            elif [ -n "${activation_target_path:-}" ] && path_exists "$activation_target_path"; then
                remove_path "$activation_target_path" || cleanup_failed=1
            fi
            if [ -n "${activation_pending_path:-}" ] && path_exists "$activation_pending_path"; then
                remove_path "$activation_pending_path" || cleanup_failed=1
            fi
            ;;
    esac

    if [ -n "${mounted_dmg:-}" ]; then
        if ! hdiutil detach -quiet "$mounted_dmg" >/dev/null 2>&1; then
            echo "Could not detach the Orion Studio disk image." >&2
            cleanup_failed=1
        fi
    fi
    if [ -n "${temp:-}" ] && [ -d "$temp" ]; then
        remove_path "$temp" || cleanup_failed=1
    fi

    if [ "$cleanup_failed" = "1" ] && [ "$original_status" -eq 0 ]; then
        original_status=1
    fi
    exit "$original_status"
}

prepare_activation() {
    activation_target_path="$1"
    activation_pending_path="$2"
    activation_backup_path="$3"
    activation_target_had_original=0

    if path_exists "$activation_pending_path" || path_exists "$activation_backup_path"; then
        echo "Installer transaction paths already exist; refusing to overwrite them." >&2
        return 1
    fi
    if path_exists "$activation_target_path"; then
        if [ ! -d "$activation_target_path" ] || [ -L "$activation_target_path" ]; then
            echo "The Orion Studio installation target is not an application directory." >&2
            return 1
        fi
        activation_target_had_original=1
    fi
    activation_state="prepared"
}

activate_pending() {
    if [ "$activation_target_had_original" = "1" ]; then
        mv "$activation_target_path" "$activation_backup_path"
        activation_state="backup_moved"
    fi

    activation_state="activating"
    if ! mv "$activation_pending_path" "$activation_target_path"; then
        echo "Failed to activate the Orion Studio installation." >&2
        return 1
    fi
    activation_state="activated"
}

commit_activation() {
    activation_state="committed"
    if path_exists "$activation_backup_path"; then
        remove_path "$activation_backup_path"
    fi
}

prepare_auxiliary_path() {
    prepare_auxiliary_target="$1"
    prepare_auxiliary_backup="$2"
    prepare_auxiliary_prepared_marker="$3"
    prepare_auxiliary_original_marker="$4"

    if path_exists "$prepare_auxiliary_target"; then
        if [ -d "$prepare_auxiliary_target" ] && [ ! -L "$prepare_auxiliary_target" ]; then
            echo "Refusing to replace an installer-owned path that is a directory: $prepare_auxiliary_target" >&2
            return 1
        fi
        : > "$prepare_auxiliary_original_marker"
    fi
    : > "$prepare_auxiliary_prepared_marker"
    if [ -f "$prepare_auxiliary_original_marker" ]; then
        mv "$prepare_auxiliary_target" "$prepare_auxiliary_backup"
    fi
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{ print $1 }'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{ print $1 }'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$1" | awk '{ print $NF }'
    else
        echo "A SHA-256 implementation is required to verify Orion Studio." >&2
        return 1
    fi
}

validate_sha256() {
    checksum_to_validate="$1"
    if [ "${#checksum_to_validate}" -ne 64 ]; then
        return 1
    fi
    case "$checksum_to_validate" in
        *[!0123456789abcdefABCDEF]*)
            return 1
            ;;
    esac
}

verify_asset_checksum() {
    checksum_asset_path="$1"
    checksum_expected="$2"
    if ! validate_sha256 "$checksum_expected"; then
        echo "The Orion Studio asset checksum is not a valid SHA-256 value." >&2
        return 1
    fi
    checksum_actual="$(sha256_file "$checksum_asset_path")" || return 1
    checksum_actual="$(printf '%s' "$checksum_actual" | tr 'ABCDEF' 'abcdef')"
    checksum_expected="$(printf '%s' "$checksum_expected" | tr 'ABCDEF' 'abcdef')"
    if [ "$checksum_actual" != "$checksum_expected" ]; then
        echo "The Orion Studio asset checksum does not match the expected SHA-256 value." >&2
        return 1
    fi
}

manifest_value() {
    manifest_value_path="$1"
    manifest_value_key="$2"
    awk -F= -v key="$manifest_value_key" '$1 == key { print substr($0, length(key) + 2) }' "$manifest_value_path"
}

validate_manifest_shape() {
    manifest_shape_path="$1"
    manifest_shape_operating_system="$2"
    awk -v expected_operating_system="$manifest_shape_operating_system" '
        NR == 1 {
            if ($0 != "orion-studio-release-manifest-v1") exit 1
            next
        }
        {
            if ($0 ~ /\r/ || $0 !~ /^[a-z0-9_]+=[^=[:space:]]+$/) exit 1
            separator = index($0, "=")
            key = substr($0, 1, separator - 1)
            if (key != "channel" && key != "requested_version" && key != "version" &&
                key != "os" && key != "arch" && key != "asset" && key != "sha256" &&
                key != "team_identifier" && key != "bundle_identifier") exit 1
            seen[key]++
            if (seen[key] != 1) exit 1
        }
        END {
            if (NR < 2 || seen["channel"] != 1 || seen["requested_version"] != 1 ||
                seen["version"] != 1 || seen["os"] != 1 || seen["arch"] != 1 ||
                seen["asset"] != 1 || seen["sha256"] != 1) exit 1
            if (expected_operating_system == "macos") {
                if (seen["team_identifier"] != 1 || seen["bundle_identifier"] != 1) exit 1
            } else if (seen["team_identifier"] != 0 || seen["bundle_identifier"] != 0) {
                exit 1
            }
        }
    ' "$manifest_shape_path"
}

copy_or_download_verification_file() {
    verification_local_path="$1"
    verification_remote_url="$2"
    verification_destination="$3"
    verification_description="$4"

    if [ -n "$verification_local_path" ] && [ -n "$verification_remote_url" ]; then
        echo "Configure only one local or remote $verification_description source." >&2
        return 1
    fi
    if [ -n "$verification_local_path" ]; then
        if [ ! -f "$verification_local_path" ]; then
            echo "The configured $verification_description does not exist: $verification_local_path" >&2
            return 1
        fi
        cp "$verification_local_path" "$verification_destination"
        return
    fi
    case "$verification_remote_url" in
        https://*)
            ;;
        *)
            echo "Remote installation requires an explicit HTTPS $verification_description URL." >&2
            return 1
            ;;
    esac
    download "$verification_remote_url" > "$verification_destination"
}

verify_release_manifest() {
    manifest_expected_operating_system="$1"
    manifest_expected_asset="$2"
    manifest_expected_bundle_identifier="$3"
    manifest_expected_team_identifier="$4"
    release_manifest_path="$temp/release-manifest"
    release_signature_path="$temp/release-manifest.sig"
    release_public_key_path="$temp/release-public-key.pem"

    copy_or_download_verification_file \
        "${ORION_STUDIO_RELEASE_MANIFEST_PATH:-}" \
        "${ORION_STUDIO_RELEASE_MANIFEST_URL:-}" \
        "$release_manifest_path" \
        "release manifest" || return 1
    copy_or_download_verification_file \
        "${ORION_STUDIO_RELEASE_MANIFEST_SIGNATURE_PATH:-}" \
        "${ORION_STUDIO_RELEASE_MANIFEST_SIGNATURE_URL:-}" \
        "$release_signature_path" \
        "release manifest signature" || return 1

    release_configured_public_key_path="${ORION_STUDIO_RELEASE_PUBLIC_KEY_PATH:-}"
    release_configured_public_key_pem="${ORION_STUDIO_RELEASE_PUBLIC_KEY_PEM:-}"
    if [ -n "$release_configured_public_key_path" ] && [ -n "$release_configured_public_key_pem" ]; then
        echo "Configure only one Orion Studio release public key source." >&2
        return 1
    fi
    if [ -n "$release_configured_public_key_path" ]; then
        if [ ! -f "$release_configured_public_key_path" ]; then
            echo "The configured Orion Studio release public key does not exist." >&2
            return 1
        fi
        cp "$release_configured_public_key_path" "$release_public_key_path"
    elif [ -n "$release_configured_public_key_pem" ]; then
        printf '%s\n' "$release_configured_public_key_pem" > "$release_public_key_path"
    else
        echo "Remote installation requires an Orion Studio release public key." >&2
        return 1
    fi

    if ! command -v openssl >/dev/null 2>&1; then
        echo "OpenSSL is required to verify the Orion Studio release manifest." >&2
        return 1
    fi
    if ! openssl dgst -sha256 -verify "$release_public_key_path" -signature "$release_signature_path" "$release_manifest_path" >/dev/null 2>&1; then
        echo "The Orion Studio release manifest signature is invalid." >&2
        return 1
    fi
    if ! validate_manifest_shape "$release_manifest_path" "$manifest_expected_operating_system"; then
        echo "The Orion Studio release manifest has an invalid schema." >&2
        return 1
    fi

    verified_manifest_channel="$(manifest_value "$release_manifest_path" channel)"
    verified_manifest_requested_version="$(manifest_value "$release_manifest_path" requested_version)"
    verified_manifest_version="$(manifest_value "$release_manifest_path" version)"
    verified_manifest_operating_system="$(manifest_value "$release_manifest_path" os)"
    verified_manifest_architecture="$(manifest_value "$release_manifest_path" arch)"
    verified_manifest_asset="$(manifest_value "$release_manifest_path" asset)"
    verified_manifest_sha256="$(manifest_value "$release_manifest_path" sha256)"

    if [ "$verified_manifest_channel" != "$channel" ] ||
        [ "$verified_manifest_requested_version" != "$orion_studio_version" ] ||
        [ "$verified_manifest_operating_system" != "$manifest_expected_operating_system" ] ||
        [ "$verified_manifest_architecture" != "$arch" ] ||
        [ "$verified_manifest_asset" != "$manifest_expected_asset" ]; then
        echo "The Orion Studio release manifest does not match the requested release." >&2
        return 1
    fi
    case "$verified_manifest_version" in
        "" | *[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._+-]*)
            echo "The Orion Studio release manifest contains an invalid version." >&2
            return 1
            ;;
    esac
    if [ "$orion_studio_version" = "latest" ] && [ "$verified_manifest_version" = "latest" ]; then
        echo "The Orion Studio release manifest did not resolve latest to an immutable version." >&2
        return 1
    fi
    if [ "$orion_studio_version" != "latest" ] && [ "$verified_manifest_version" != "$orion_studio_version" ]; then
        echo "The Orion Studio release manifest resolved a different version." >&2
        return 1
    fi
    if ! validate_sha256 "$verified_manifest_sha256"; then
        echo "The Orion Studio release manifest contains an invalid asset checksum." >&2
        return 1
    fi

    if [ "$manifest_expected_operating_system" = "macos" ]; then
        verified_manifest_team_identifier="$(manifest_value "$release_manifest_path" team_identifier)"
        verified_manifest_bundle_identifier="$(manifest_value "$release_manifest_path" bundle_identifier)"
        if [ "$verified_manifest_team_identifier" != "$manifest_expected_team_identifier" ] ||
            [ "$verified_manifest_bundle_identifier" != "$manifest_expected_bundle_identifier" ]; then
            echo "The Orion Studio release manifest contains an unexpected macOS identity." >&2
            return 1
        fi
    fi
}

obtain_asset() {
    obtain_destination="$1"
    obtain_expected_asset="$2"
    obtain_expected_operating_system="$3"
    obtain_asset_url="$4"
    obtain_expected_bundle_identifier="$5"
    obtain_expected_team_identifier="$6"

    if [ -n "$orion_studio_bundle_path" ]; then
        obtain_local_bundle_checksum="${ORION_STUDIO_BUNDLE_SHA256:-}"
        if [ -z "$obtain_local_bundle_checksum" ]; then
            echo "A local Orion Studio bundle requires ORION_STUDIO_BUNDLE_SHA256." >&2
            return 1
        fi
        cp "$orion_studio_bundle_path" "$obtain_destination"
        verify_asset_checksum "$obtain_destination" "$obtain_local_bundle_checksum"
        return
    fi

    verify_release_manifest \
        "$obtain_expected_operating_system" \
        "$obtain_expected_asset" \
        "$obtain_expected_bundle_identifier" \
        "$obtain_expected_team_identifier" || return 1
    download "$obtain_asset_url" > "$obtain_destination"
    verify_asset_checksum "$obtain_destination" "$verified_manifest_sha256"
}

verify_macos_identity() {
    identity_app_path="$1"
    identity_expected_bundle_identifier="$2"
    identity_expected_team_identifier="$3"
    identity_signature_details="$(codesign -d --verbose=4 "$identity_app_path" 2>&1)" || return 1
    identity_actual_bundle_identifier="$(printf '%s\n' "$identity_signature_details" | awk -F= '$1 == "Identifier" { print substr($0, 12) }')"
    identity_actual_team_identifier="$(printf '%s\n' "$identity_signature_details" | awk -F= '$1 == "TeamIdentifier" { print substr($0, 16) }')"
    if [ "$identity_actual_bundle_identifier" != "$identity_expected_bundle_identifier" ] ||
        [ "$identity_actual_team_identifier" != "$identity_expected_team_identifier" ]; then
        echo "The Orion Studio application has an unexpected publisher identity." >&2
        return 1
    fi
}

main() {
    platform="$(uname -s)"
    arch="$(uname -m)"
    channel="${ORION_STUDIO_CHANNEL:-${ZED_CHANNEL:-stable}}"
    orion_studio_version="${ORION_STUDIO_VERSION:-${ZED_VERSION:-latest}}"
    orion_studio_bundle_path="${ORION_STUDIO_BUNDLE_PATH:-${ZED_BUNDLE_PATH:-}}"
    applications_directory="/Applications"
    case "${HOME:-}" in
        "" | /)
            echo "HOME must identify a non-root user directory." >&2
            exit 1
            ;;
    esac
    case "$channel" in
        stable | preview | nightly | dev)
            ;;
        *)
            echo "Unsupported Orion Studio release channel: $channel" >&2
            exit 1
            ;;
    esac
    case "$orion_studio_version" in
        "" | *[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._+-]*)
            echo "Unsupported Orion Studio version: $orion_studio_version" >&2
            exit 1
            ;;
    esac
    # Use TMPDIR if available (for environments with non-standard temp directories)
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        temp="$(mktemp -d "$TMPDIR/orion-studio-XXXXXX")"
    else
        temp="$(mktemp -d "/tmp/orion-studio-XXXXXX")"
    fi
    mounted_dmg=""
    activation_state="idle"
    activation_target_path=""
    activation_pending_path=""
    activation_backup_path=""
    activation_target_had_original=0
    transaction_directory="$temp/transaction"
    mkdir -p "$transaction_directory"
    canonical_cli_link_path="$HOME/.local/bin/orion-studio"
    canonical_cli_link_backup="$transaction_directory/orion-studio-cli.previous"
    canonical_cli_link_prepared_marker="$transaction_directory/orion-studio-cli.prepared"
    canonical_cli_link_original_marker="$transaction_directory/orion-studio-cli.original"
    short_cli_link_path="$HOME/.local/bin/orion"
    short_cli_link_backup="$transaction_directory/orion-cli.previous"
    short_cli_link_prepared_marker="$transaction_directory/orion-cli.prepared"
    short_cli_link_original_marker="$transaction_directory/orion-cli.original"
    legacy_cli_link_path="$HOME/.local/bin/zed"
    legacy_cli_link_backup="$transaction_directory/legacy-cli.previous"
    legacy_cli_link_prepared_marker="$transaction_directory/legacy-cli.prepared"
    legacy_cli_link_original_marker="$transaction_directory/legacy-cli.original"
    desktop_file_path=""
    desktop_file_backup="$transaction_directory/desktop-file.previous"
    desktop_file_prepared_marker="$transaction_directory/desktop-file.prepared"
    desktop_file_original_marker="$transaction_directory/desktop-file.original"
    trap cleanup EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM

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
        download () {
            command curl -fL --proto '=https' --proto-redir '=https' "$@"
        }
    elif command -v wget >/dev/null 2>&1; then
        download () {
            wget --https-only -O- "$@"
        }
    elif [ -z "$orion_studio_bundle_path" ]; then
        echo "Could not find a secure HTTPS downloader in your path"
        exit 1
    fi

    "$platform" "$@"

    if [ "$(command -v orion-studio)" = "$HOME/.local/bin/orion-studio" ]; then
        echo "Orion Studio has been installed. Run with 'orion-studio'"
    else
        echo "To run Orion Studio from your terminal, you must add ~/.local/bin to your PATH"
        echo "Run:"

        case "${SHELL:-}" in
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
    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    case "$channel" in
      stable)
        appid="com.orion.OrionStudio"
        ;;
      nightly)
        appid="com.orion.OrionStudio.Nightly"
        ;;
      preview)
        appid="com.orion.OrionStudio.Preview"
        ;;
      dev)
        appid="com.orion.OrionStudio.Dev"
        ;;
    esac

    archive="$temp/orion-studio-linux-$arch.tar.gz"
    asset_name="orion-studio-linux-$arch.tar.gz"
    asset_url="https://cloud.orion.dev/releases/$channel/$orion_studio_version/download?asset=orion-studio&arch=$arch&os=linux&source=install.sh"
    if [ -z "$orion_studio_bundle_path" ]; then
        echo "Downloading verified Orion Studio version: $orion_studio_version"
    fi
    obtain_asset "$archive" "$asset_name" linux "$asset_url" "" ""

    expected_root="orion-studio$suffix.app"
    extracted="$temp/extracted"
    mkdir -p "$extracted"

    if ! tar -tzf "$archive" | awk -v root="$expected_root" '
        function unsafe(path, component_count, component_index, components) {
            if (substr(path, 1, 1) == "/") return 1
            component_count = split(path, components, "/")
            for (component_index = 1; component_index <= component_count; component_index++) {
                if (components[component_index] == "..") return 1
            }
            return 0
        }
        unsafe($0) { exit 1 }
        $0 == root || $0 == root "/" || index($0, root "/") == 1 { next }
        { exit 1 }
    '; then
        echo "The Orion Studio archive contains entries outside $expected_root." >&2
        return 1
    fi
    tar -xzf "$archive" -C "$extracted"

    staged_app="$extracted/$expected_root"
    bundle_cli="$staged_app/bin/orion-studio"
    if [ ! -x "$staged_app/libexec/orion-studio" ] || [ ! -x "$bundle_cli" ]; then
        echo "The Orion Studio archive is missing its required executables." >&2
        return 1
    fi
    if ! find "$staged_app" -type l -exec sh -c '
        for link_path do
            link_target=$(readlink "$link_path") || exit 1
            case "$link_target" in
                "" | /* | *../* | ../* | */.. | ..) exit 1 ;;
            esac
        done
    ' sh {} +; then
        echo "The Orion Studio archive contains an unsafe symlink." >&2
        return 1
    fi

    mkdir -p "$HOME/.local"
    target_app="$HOME/.local/$expected_root"
    pending_app="$HOME/.local/.${expected_root}.new.$$"
    backup_app="$HOME/.local/.${expected_root}.backup.$$"
    prepare_activation "$target_app" "$pending_app" "$backup_app"
    mv "$staged_app" "$pending_app"
    activate_pending

    orion_studio_executable="$target_app/libexec/orion-studio"
    if [ -x "$orion_studio_executable" ] && command -v ldd >/dev/null 2>&1; then
        missing="$(ldd "$orion_studio_executable" 2>/dev/null | sed -n 's/^[[:space:]]*\(.*\) => not found$/\1/p')"
        if [ -n "$missing" ]; then
            echo "Warning: your system is missing libraries that Orion Studio needs:"
            echo "$missing" | sed 's/^/    /'
            echo "Install them with your package manager, or Orion Studio will fail to start."
        fi
    fi

    # Setup ~/.local directories
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"

    bundle_cli="$target_app/bin/orion-studio"
    if [ ! -x "$bundle_cli" ]; then
        echo "The activated Orion Studio installation has no CLI executable." >&2
        return 1
    fi

    desktop_file_path="$HOME/.local/share/applications/${appid}.desktop"
    src_dir="$target_app/share/applications"
    if [ -f "$src_dir/${appid}.desktop" ]; then
        desktop_file_source="$src_dir/${appid}.desktop"
    elif [ -f "$src_dir/orion-studio$suffix.desktop" ]; then
        desktop_file_source="$src_dir/orion-studio$suffix.desktop"
    else
        echo "The Orion Studio archive is missing its desktop entry." >&2
        return 1
    fi
    desktop_file_staged="$transaction_directory/desktop-file.new"
    sed \
        -e "s|Icon=orion-studio|Icon=$target_app/share/icons/hicolor/512x512/apps/orion-studio.png|g" \
        -e "s|Exec=orion-studio|Exec=$bundle_cli|g" \
        -e "s|Exec=orion|Exec=$bundle_cli|g" \
        "$desktop_file_source" > "$desktop_file_staged"

    prepare_auxiliary_path \
        "$canonical_cli_link_path" \
        "$canonical_cli_link_backup" \
        "$canonical_cli_link_prepared_marker" \
        "$canonical_cli_link_original_marker"
    ln -s "$bundle_cli" "$canonical_cli_link_path"
    prepare_auxiliary_path \
        "$short_cli_link_path" \
        "$short_cli_link_backup" \
        "$short_cli_link_prepared_marker" \
        "$short_cli_link_original_marker"
    ln -s "$canonical_cli_link_path" "$short_cli_link_path"
    prepare_auxiliary_path \
        "$legacy_cli_link_path" \
        "$legacy_cli_link_backup" \
        "$legacy_cli_link_prepared_marker" \
        "$legacy_cli_link_original_marker"
    ln -s "$canonical_cli_link_path" "$legacy_cli_link_path"
    prepare_auxiliary_path \
        "$desktop_file_path" \
        "$desktop_file_backup" \
        "$desktop_file_prepared_marker" \
        "$desktop_file_original_marker"
    cp "$desktop_file_staged" "$desktop_file_path"

    commit_activation
}

macos() {
    expected_team_identifier="${ORION_STUDIO_MACOS_TEAM_ID:-}"
    if [ -z "$expected_team_identifier" ]; then
        echo "macOS installation requires ORION_STUDIO_MACOS_TEAM_ID." >&2
        return 1
    fi
    case "$expected_team_identifier" in
        *[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789]*)
            echo "ORION_STUDIO_MACOS_TEAM_ID contains invalid characters." >&2
            return 1
            ;;
    esac

    case "$channel" in
        stable)
            app="Orion Studio.app"
            expected_bundle_identifier="dev.orion.OrionStudio"
            ;;
        preview)
            app="Orion Studio Preview.app"
            expected_bundle_identifier="dev.orion.OrionStudio-Preview"
            ;;
        nightly)
            app="Orion Studio Nightly.app"
            expected_bundle_identifier="dev.orion.OrionStudio-Nightly"
            ;;
        dev)
            app="Orion Studio Dev.app"
            expected_bundle_identifier="dev.orion.OrionStudio-Dev"
            ;;
    esac

    dmg_path="$temp/Orion-Studio-$arch.dmg"
    asset_name="Orion-Studio-$arch.dmg"
    asset_url="https://cloud.orion.dev/releases/$channel/$orion_studio_version/download?asset=orion-studio&os=macos&arch=$arch&source=install.sh"
    if [ -z "$orion_studio_bundle_path" ]; then
        echo "Downloading verified Orion Studio version: $orion_studio_version"
    fi
    obtain_asset \
        "$dmg_path" \
        "$asset_name" \
        macos \
        "$asset_url" \
        "$expected_bundle_identifier" \
        "$expected_team_identifier"

    mounted_dmg="$temp/mount"
    hdiutil attach -quiet "$dmg_path" -mountpoint "$mounted_dmg"
    mounted_app="$mounted_dmg/$app"
    if [ ! -d "$mounted_app" ] ||
        [ ! -x "$mounted_app/Contents/MacOS/orion-studio" ] ||
        [ ! -x "$mounted_app/Contents/MacOS/cli" ]; then
        echo "The Orion Studio disk image does not contain the expected $app bundle." >&2
        return 1
    fi
    codesign --verify --deep --strict "$mounted_app"
    verify_macos_identity "$mounted_app" "$expected_bundle_identifier" "$expected_team_identifier"
    spctl --assess --type execute "$mounted_app"
    echo "Installing $app"
    target_app="$applications_directory/$app"
    pending_app="$applications_directory/.${app}.new.$$"
    backup_app="$applications_directory/.${app}.backup.$$"
    prepare_activation "$target_app" "$pending_app" "$backup_app"
    ditto "$mounted_app" "$pending_app"
    if [ ! -x "$pending_app/Contents/MacOS/orion-studio" ] ||
        [ ! -x "$pending_app/Contents/MacOS/cli" ]; then
        echo "The copied Orion Studio application is missing a required executable." >&2
        return 1
    fi
    codesign --verify --deep --strict "$pending_app"
    verify_macos_identity "$pending_app" "$expected_bundle_identifier" "$expected_team_identifier"
    spctl --assess --type execute "$pending_app"
    activate_pending
    codesign --verify --deep --strict "$target_app"
    verify_macos_identity "$target_app" "$expected_bundle_identifier" "$expected_team_identifier"
    spctl --assess --type execute "$target_app"
    hdiutil detach -quiet "$mounted_dmg"
    mounted_dmg=""

    mkdir -p "$HOME/.local/bin"
    prepare_auxiliary_path \
        "$canonical_cli_link_path" \
        "$canonical_cli_link_backup" \
        "$canonical_cli_link_prepared_marker" \
        "$canonical_cli_link_original_marker"
    ln -s "$target_app/Contents/MacOS/cli" "$canonical_cli_link_path"
    prepare_auxiliary_path \
        "$short_cli_link_path" \
        "$short_cli_link_backup" \
        "$short_cli_link_prepared_marker" \
        "$short_cli_link_original_marker"
    ln -s "$canonical_cli_link_path" "$short_cli_link_path"
    prepare_auxiliary_path \
        "$legacy_cli_link_path" \
        "$legacy_cli_link_backup" \
        "$legacy_cli_link_prepared_marker" \
        "$legacy_cli_link_original_marker"
    ln -s "$canonical_cli_link_path" "$legacy_cli_link_path"

    commit_activation
}

main "$@"
