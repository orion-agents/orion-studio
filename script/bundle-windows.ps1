[CmdletBinding()]
Param(
    [Parameter()][Alias('i')][switch]$Install,
    [Parameter()][Alias('h')][switch]$Help,
    [Parameter()][Alias('a')][string]$Architecture
)

. "$PSScriptRoot/lib/workspace.ps1"

# https://stackoverflow.com/questions/57949031/powershell-script-stops-if-program-fails-like-bash-set-o-errexit
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$buildSuccess = $false
$canCodeSign = $false
$unsignedDevBuild = $false
$requireSignedRelease = $false
$workspace = $null
$publisherDisplayName = $null
$appxPublisher = $null
$installerPath = $null
$stagedCliExecutableName = "orion-studio-cli.exe"
$canonicalCliExecutableName = "orion-studio.exe"
$shortCliExecutableName = "orion.exe"
$legacyCliExecutableName = "zed.exe"
$canonicalCliShellName = "orion-studio"
$shortCliShellName = "orion"
$legacyCliShellName = "zed"

$OSArchitecture = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    "X64" { "x86_64" }
    "Arm64" { "aarch64" }
    default { throw "Unsupported architecture" }
}

$Architecture = if ($Architecture) {
    $Architecture
} else {
    $OSArchitecture
}

if ($Architecture -notin @("x86_64", "aarch64")) {
    throw "Unsupported architecture '$Architecture'. Expected x86_64 or aarch64."
}

$CargoOutDir = "./target/$Architecture-pc-windows-msvc/release"

function Get-VSArch {
    param(
        [string]$Arch
    )

    switch ($Arch) {
        "x86_64" { "amd64" }
        "aarch64" { "arm64" }
        default { throw "Unsupported Visual Studio architecture '$Arch'." }
    }
}

$target = "$Architecture-pc-windows-msvc"

if ($Help) {
    Write-Output "Usage: bundle-windows.ps1 [-Architecture <x86_64|aarch64>] [-Install] [-Help]"
    Write-Output "Build the installer for Windows.\n"
    Write-Output "Options:"
    Write-Output "  -Architecture, -a Which architecture to build (x86_64 or aarch64)"
    Write-Output "  -Install, -i      Run the installer after building."
    Write-Output "  -Help, -h         Show this help message."
    exit 0
}

Push-Location
& "C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Launch-VsDevShell.ps1" -Arch (Get-VSArch -Arch $Architecture) -HostArch (Get-VSArch -Arch $OSArchitecture)
Pop-Location

Push-Location -Path crates/zed
$channel = (Get-Content "RELEASE_CHANNEL" -Raw).Trim()
$env:ORION_STUDIO_RELEASE_CHANNEL = $channel
# Legacy build scripts still consume this during the compatibility window.
$env:ZED_RELEASE_CHANNEL = $channel
$env:RELEASE_CHANNEL = $channel
Pop-Location

function Get-RequiredEnvironmentVariable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$VariableName
    )

    $value = [Environment]::GetEnvironmentVariable($VariableName)
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "$VariableName is required."
    }

    return $value
}

function Get-BinaryEnvironmentFlag {
    param(
        [Parameter(Mandatory = $true)]
        [string]$VariableName,
        [Parameter()]
        [string]$LegacyVariableName,
        [Parameter(Mandatory = $true)]
        [ValidateSet("0", "1")]
        [string]$DefaultValue
    )

    $resolvedVariableName = $VariableName
    $value = [Environment]::GetEnvironmentVariable($VariableName)
    if ($null -eq $value -and (-not [string]::IsNullOrWhiteSpace($LegacyVariableName))) {
        $legacyValue = [Environment]::GetEnvironmentVariable($LegacyVariableName)
        if ($null -ne $legacyValue) {
            $resolvedVariableName = $LegacyVariableName
            $value = $legacyValue
        }
    }
    if ($null -eq $value) {
        $value = $DefaultValue
    }
    if ($value -notin @("0", "1")) {
        throw "$resolvedVariableName must be exactly 0 or 1."
    }

    return $value -eq "1"
}

function CheckEnvironmentVariables {
    $script:workspace = $env:ORION_STUDIO_WORKSPACE
    if ([string]::IsNullOrWhiteSpace($script:workspace)) {
        # ParseZedWorkspace is retained as a compatibility adapter until the shared helper is renamed.
        $script:workspace = $env:ZED_WORKSPACE
    }
    if ([string]::IsNullOrWhiteSpace($script:workspace)) {
        throw "ORION_STUDIO_WORKSPACE is required."
    }
    $env:ORION_STUDIO_WORKSPACE = $script:workspace
    $env:ZED_WORKSPACE = $script:workspace

    if ([string]::IsNullOrWhiteSpace($env:RELEASE_VERSION)) {
        $resolvedReleaseVersion = & "$PSScriptRoot\get-crate-version.ps1" orion-studio
        if (-not $? -or [string]::IsNullOrWhiteSpace($resolvedReleaseVersion)) {
            throw "Unable to resolve the orion-studio release version."
        }
        $env:RELEASE_VERSION = $resolvedReleaseVersion.Trim()
    }

    $script:publisherDisplayName = Get-RequiredEnvironmentVariable -VariableName "ORION_STUDIO_WINDOWS_PUBLISHER"
    $script:appxPublisher = Get-RequiredEnvironmentVariable -VariableName "ORION_STUDIO_WINDOWS_APPX_PUBLISHER"
    if ($script:publisherDisplayName -match '[\r\n"]') {
        throw "ORION_STUDIO_WINDOWS_PUBLISHER contains characters that cannot be passed to Inno Setup."
    }

    $script:requireSignedRelease = Get-BinaryEnvironmentFlag `
        -VariableName "ORION_STUDIO_REQUIRE_SIGNED_RELEASE" `
        -LegacyVariableName "ORION_STUDIO_WINDOWS_REQUIRE_SIGNING" `
        -DefaultValue "0"

    $signingVars = @(
        'AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET',
        'ACCOUNT_NAME', 'CERT_PROFILE_NAME', 'ENDPOINT',
        'FILE_DIGEST', 'TIMESTAMP_DIGEST', 'TIMESTAMP_SERVER'
    )

    $missingVars = @($signingVars | Where-Object { [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_)) })
    if ($missingVars.Count -eq 0) {
        $script:canCodeSign = $true
    } else {
        if ($channel -ne "dev") {
            throw "Windows signing is required for the '$channel' channel, but these variables are missing: $($missingVars -join ', ')"
        }
        if ($script:requireSignedRelease) {
            throw "Windows signing was explicitly required, but these variables are missing: $($missingVars -join ', ')"
        }
        if ($env:ORION_STUDIO_WINDOWS_ALLOW_UNSIGNED_DEV -ne "1") {
            throw "Unsigned Windows artifacts are restricted to the dev channel and require ORION_STUDIO_WINDOWS_ALLOW_UNSIGNED_DEV=1. Missing signing variables: $($missingVars -join ', ')"
        }
        $script:unsignedDevBuild = $true
        Write-Warning "Building an explicitly opted-in unsigned Orion Studio Dev artifact. It must not be promoted to Preview, Nightly, or Stable."
    }
}

function PrepareForBundle {
    if (Test-Path "$innoDir") {
        Remove-Item -Path "$innoDir" -Recurse -Force
    }
    New-Item -Path "$innoDir" -ItemType Directory -Force
    Copy-Item -Path "$workspace\crates\zed\resources\windows\*" -Destination "$innoDir" -Recurse -Force
    if (-not $unsignedDevBuild) {
        New-Item -Path "$innoDir\make_appx" -ItemType Directory -Force
        New-Item -Path "$innoDir\appx" -ItemType Directory -Force
    }
    New-Item -Path "$innoDir\bin" -ItemType Directory -Force
    New-Item -Path "$innoDir\tools" -ItemType Directory -Force

    rustup target add $target
}

function GenerateLicenses {
    . $PSScriptRoot/generate-licenses.ps1
}

function BuildOrionStudioAndTools {
    Write-Output "Building Orion Studio and supporting tools for channel: $channel"
    cargo build --release --package orion-studio --package cli --package auto_update_helper --target $target
    Copy-Item -Path ".\$CargoOutDir\orion-studio.exe" -Destination "$innoDir\orion-studio.exe" -Force
    # Keep the staged CLI distinct from the root GUI executable until it is moved into bin.
    Copy-Item -Path ".\$CargoOutDir\cli.exe" -Destination "$innoDir\$stagedCliExecutableName" -Force
    Copy-Item -Path ".\$CargoOutDir\auto_update_helper.exe" -Destination "$innoDir\auto_update_helper.exe" -Force
    if (-not $unsignedDevBuild) {
        switch ($channel) {
            "stable" {
                cargo build --release --features stable --no-default-features --package explorer_command_injector --target $target
            }
            "preview" {
                cargo build --release --features preview --no-default-features --package explorer_command_injector --target $target
            }
            "nightly" {
                cargo build --release --features nightly --no-default-features --package explorer_command_injector --target $target
            }
            "dev" {
                cargo build --release --features dev --no-default-features --package explorer_command_injector --target $target
            }
            default {
                throw "Unsupported release channel '$channel'."
            }
        }
        Copy-Item -Path ".\$CargoOutDir\explorer_command_injector.dll" -Destination "$innoDir\orion_studio_explorer_command_injector.dll" -Force
    }
}

function BuildRemoteServer {
    Write-Output "Building remote_server for $target"
    cargo build --release --package remote_server --target $target

    # Create zipped remote server binary
    $remoteServerSrc = (Resolve-Path ".\$CargoOutDir\remote_server.exe").Path

    if ($canCodeSign) {
        Write-Output "Code signing remote_server.exe"
        & "$innoDir\sign.ps1" $remoteServerSrc
        Assert-SignedByConfiguredPublisher -Files @($remoteServerSrc)
    }

    $remoteServerDst = "$workspace\target\orion-studio-remote-server-windows-$Architecture.zip"
    Write-Output "Compressing remote_server to $remoteServerDst"
    Compress-Archive -Path $remoteServerSrc -DestinationPath $remoteServerDst -Force

    Write-Output "Remote server compressed successfully"
}

function ZipOrionStudioDebugSymbols {
    $items = @(
        ".\$CargoOutDir\orion-studio.pdb",
        ".\$CargoOutDir\cli.pdb",
        ".\$CargoOutDir\auto_update_helper.pdb",
        ".\$CargoOutDir\remote_server.pdb"
    )
    if (-not $unsignedDevBuild) {
        $items += ".\$CargoOutDir\explorer_command_injector.pdb"
    }

    Compress-Archive -Path $items -DestinationPath $debugArchive -Force
}


function UploadToSentry {
    if (-not (Get-Command "sentry-cli" -ErrorAction SilentlyContinue)) {
        Write-Output "sentry-cli not found. skipping sentry upload."
        Write-Output "install with: 'winget install -e --id=Sentry.sentry-cli'"
        return
    }
    if ([string]::IsNullOrWhiteSpace($env:SENTRY_AUTH_TOKEN)) {
        Write-Output "missing SENTRY_AUTH_TOKEN. skipping sentry upload."
        return
    }
    $sentryProject = $env:ORION_STUDIO_SENTRY_PROJECT
    $sentryOrganization = $env:ORION_STUDIO_SENTRY_ORGANIZATION
    if ([string]::IsNullOrWhiteSpace($sentryProject) -or [string]::IsNullOrWhiteSpace($sentryOrganization)) {
        Write-Output "Orion Studio Sentry project or organization is not configured. Skipping Sentry upload."
        return
    }
    Write-Output "Uploading Orion Studio debug symbols to Sentry..."
    for ($i = 1; $i -le 3; $i++) {
        try {
            sentry-cli debug-files upload --include-sources --wait -p $sentryProject -o $sentryOrganization $CargoOutDir
            break
        }
        catch {
            Write-Output "Sentry upload attempt $i failed: $_"
            if ($i -eq 3) {
                Write-Output "All sentry upload attempts failed"
                throw
            }
            Start-Sleep -Seconds 2
        }
    }
}

function Get-AppxVersion {
    $numericVersion = ($env:RELEASE_VERSION -split '[-+]')[0]
    $versionParts = @($numericVersion.Split('.'))
    if ($versionParts.Count -gt 4) {
        throw "RELEASE_VERSION '$env:RELEASE_VERSION' cannot be represented as an AppX version."
    }
    while ($versionParts.Count -lt 4) {
        $versionParts += "0"
    }
    foreach ($versionPart in $versionParts) {
        [uint16]$parsedVersionPart = 0
        if (-not [uint16]::TryParse($versionPart, [ref]$parsedVersionPart)) {
            throw "RELEASE_VERSION '$env:RELEASE_VERSION' cannot be represented as an AppX version."
        }
    }
    return $versionParts -join '.'
}

function Get-PngDimensions {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $pngSignature = [byte[]](137, 80, 78, 71, 13, 10, 26, 10)
    if ($bytes.Length -lt 24) {
        throw "AppX logo '$Path' is too short to be a PNG."
    }
    for ($index = 0; $index -lt $pngSignature.Length; $index++) {
        if ($bytes[$index] -ne $pngSignature[$index]) {
            throw "AppX logo '$Path' does not have a valid PNG signature."
        }
    }
    if ([System.Text.Encoding]::ASCII.GetString($bytes, 12, 4) -ne "IHDR") {
        throw "AppX logo '$Path' does not contain an IHDR header at the expected location."
    }

    $width = ([uint32]$bytes[16] -shl 24) -bor ([uint32]$bytes[17] -shl 16) -bor ([uint32]$bytes[18] -shl 8) -bor [uint32]$bytes[19]
    $height = ([uint32]$bytes[20] -shl 24) -bor ([uint32]$bytes[21] -shl 16) -bor ([uint32]$bytes[22] -shl 8) -bor [uint32]$bytes[23]
    return [PSCustomObject]@{
        Width = $width
        Height = $height
    }
}

function Assert-AppxLogo {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [uint32]$ExpectedWidth,
        [Parameter(Mandatory = $true)]
        [uint32]$ExpectedHeight
    )

    if (-not (Test-Path -Path $Path -PathType Leaf)) {
        throw "AppX logo resource '$Path' is required."
    }
    $dimensions = Get-PngDimensions -Path $Path
    if ($dimensions.Width -ne $ExpectedWidth -or $dimensions.Height -ne $ExpectedHeight) {
        throw "AppX logo '$Path' must be ${ExpectedWidth}x${ExpectedHeight}, but is $($dimensions.Width)x$($dimensions.Height)."
    }
}

function MakeAppx {
    if ($unsignedDevBuild) {
        Write-Output "Skipping the AppX package for the unsigned dev installer; classic context-menu registration will be used."
        return
    }

    switch ($channel) {
        "stable" {
            $manifestFile = "$workspace\crates\explorer_command_injector\AppxManifest.xml"
        }
        "preview" {
            $manifestFile = "$workspace\crates\explorer_command_injector\AppxManifest-Preview.xml"
        }
        "nightly" {
            $manifestFile = "$workspace\crates\explorer_command_injector\AppxManifest-Nightly.xml"
        }
        "dev" {
            $manifestFile = "$workspace\crates\explorer_command_injector\AppxManifest-Dev.xml"
        }
        default {
            throw "Unsupported release channel '$channel'."
        }
    }
    $stagedManifest = "$innoDir\make_appx\AppxManifest.xml"
    Copy-Item -Path $manifestFile -Destination $stagedManifest
    $appxResources = "$workspace\crates\explorer_command_injector\resources"
    Assert-AppxLogo -Path "$appxResources\logo_50x50.png" -ExpectedWidth 50 -ExpectedHeight 50
    Assert-AppxLogo -Path "$appxResources\logo_150x150.png" -ExpectedWidth 150 -ExpectedHeight 150
    Assert-AppxLogo -Path "$appxResources\logo_44x44.png" -ExpectedWidth 44 -ExpectedHeight 44
    Copy-Item -Path $appxResources -Destination "$innoDir\make_appx\resources" -Recurse -Force

    $manifestContent = Get-Content -Path $stagedManifest -Raw
    $manifestContent = $manifestContent.Replace(
        "__ORION_STUDIO_WINDOWS_APPX_PUBLISHER__",
        [System.Security.SecurityElement]::Escape($appxPublisher)
    )
    $manifestContent = $manifestContent.Replace(
        "__ORION_STUDIO_WINDOWS_PUBLISHER__",
        [System.Security.SecurityElement]::Escape($publisherDisplayName)
    )
    $manifestContent = $manifestContent.Replace(
        "__ORION_STUDIO_WINDOWS_APPX_VERSION__",
        (Get-AppxVersion)
    )
    if ($manifestContent.Contains("__ORION_STUDIO_")) {
        throw "The staged AppX manifest contains unresolved Orion Studio placeholders."
    }
    [System.IO.File]::WriteAllText($stagedManifest, $manifestContent, [System.Text.UTF8Encoding]::new($false))

    # Add makeAppx.exe to Path
    $sdk = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64"
    $env:Path += ';' + $sdk
    makeAppx.exe pack /d "$innoDir\make_appx" /p "$innoDir\orion_studio_explorer_command_injector.appx" /nv
    if ($LASTEXITCODE -ne 0) {
        throw "makeAppx.exe failed with exit code $LASTEXITCODE."
    }
}

function Assert-SignedByConfiguredPublisher {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Files
    )

    foreach ($file in $Files) {
        $signature = Get-AuthenticodeSignature -FilePath $file
        if ($signature.Status -ne "Valid") {
            throw "Authenticode signature verification failed for '$file': $($signature.StatusMessage)"
        }
        if ($null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -ne $appxPublisher) {
            throw "The signer for '$file' does not match ORION_STUDIO_WINDOWS_APPX_PUBLISHER."
        }
    }
}

function SignOrionStudioAndTools {
    if (-not $canCodeSign) {
        return
    }

    $files = @(
        "$innoDir\orion-studio.exe",
        "$innoDir\$stagedCliExecutableName",
        "$innoDir\auto_update_helper.exe",
        "$innoDir\orion_studio_explorer_command_injector.dll",
        "$innoDir\orion_studio_explorer_command_injector.appx"
    )
    & "$innoDir\sign.ps1" ($files -join ',')
    Assert-SignedByConfiguredPublisher -Files $files
}

function DownloadAMDGpuServices {
    # If you update the AGS SDK version, please also update the version in `crates/gpui/src/platform/windows/directx_renderer.rs`
    $url = "https://codeload.github.com/GPUOpen-LibrariesAndSDKs/AGS_SDK/zip/refs/tags/v6.3.0"
    $zipPath = ".\AGS_SDK_v6.3.0.zip"
    # Download the AGS SDK zip file
    Invoke-WebRequest -Uri $url -OutFile $zipPath
    # Extract the AGS SDK zip file
    Expand-Archive -Path $zipPath -DestinationPath "." -Force
}

function DownloadConpty {
    $url = "https://github.com/microsoft/terminal/releases/download/v1.23.13503.0/Microsoft.Windows.Console.ConPTY.1.23.251216003.nupkg"
    $zipPath = ".\Microsoft.Windows.Console.ConPTY.1.23.251216003.nupkg"
    Invoke-WebRequest -Uri $url -OutFile $zipPath
    Expand-Archive -Path $zipPath -DestinationPath ".\conpty" -Force
}

function CollectFiles {
    if (-not $unsignedDevBuild) {
        Move-Item -Path "$innoDir\orion_studio_explorer_command_injector.appx" -Destination "$innoDir\appx\orion_studio_explorer_command_injector.appx" -Force
        Move-Item -Path "$innoDir\orion_studio_explorer_command_injector.dll" -Destination "$innoDir\appx\orion_studio_explorer_command_injector.dll" -Force
    }
    Move-Item -Path "$innoDir\$stagedCliExecutableName" -Destination "$innoDir\bin\$canonicalCliExecutableName" -Force
    Copy-Item -Path "$innoDir\bin\$canonicalCliExecutableName" -Destination "$innoDir\bin\$shortCliExecutableName" -Force
    Copy-Item -Path "$innoDir\bin\$canonicalCliExecutableName" -Destination "$innoDir\bin\$legacyCliExecutableName" -Force

    $cliShellWrapper = "$innoDir\orion-studio.sh"
    $cliShellWrapperContent = [System.IO.File]::ReadAllText($cliShellWrapper)
    if (-not $cliShellWrapperContent.Contains("orion-studio.exe")) {
        throw "The Windows CLI shell wrapper does not reference the expected staged executable."
    }
    [System.IO.File]::WriteAllText($cliShellWrapper, $cliShellWrapperContent, [System.Text.UTF8Encoding]::new($false))
    Move-Item -Path $cliShellWrapper -Destination "$innoDir\bin\$canonicalCliShellName" -Force
    Copy-Item -Path "$innoDir\bin\$canonicalCliShellName" -Destination "$innoDir\bin\$shortCliShellName" -Force
    Copy-Item -Path "$innoDir\bin\$canonicalCliShellName" -Destination "$innoDir\bin\$legacyCliShellName" -Force
    Move-Item -Path "$innoDir\auto_update_helper.exe" -Destination "$innoDir\tools\auto_update_helper.exe" -Force
    if($Architecture -eq "aarch64") {
        New-Item -Type Directory -Path "$innoDir\arm64" -Force
        Move-Item -Path ".\conpty\build\native\runtimes\arm64\OpenConsole.exe" -Destination "$innoDir\arm64\OpenConsole.exe" -Force
        Move-Item -Path ".\conpty\runtimes\win-arm64\native\conpty.dll" -Destination "$innoDir\conpty.dll" -Force
    }
    else {
        New-Item -Type Directory -Path "$innoDir\x64" -Force
        New-Item -Type Directory -Path "$innoDir\arm64" -Force
        Move-Item -Path ".\AGS_SDK-6.3.0\ags_lib\lib\amd_ags_x64.dll" -Destination "$innoDir\amd_ags_x64.dll" -Force
        Move-Item -Path ".\conpty\build\native\runtimes\x64\OpenConsole.exe" -Destination "$innoDir\x64\OpenConsole.exe" -Force
        Move-Item -Path ".\conpty\build\native\runtimes\arm64\OpenConsole.exe" -Destination "$innoDir\arm64\OpenConsole.exe" -Force
        Move-Item -Path ".\conpty\runtimes\win-x64\native\conpty.dll" -Destination "$innoDir\conpty.dll" -Force
    }
}

function BuildInstaller {
    $issFilePath = "$innoDir\orion-studio.iss"
    switch ($channel) {
        "stable" {
            $appId = "OrionStudio-Stable"
            $legacyAppId = "{{2DB0DA96-CA55-49BB-AF4F-64AF36A86712}"
            $appIconName = "app-icon"
            $appName = "Orion Studio"
            $appDisplayName = "Orion Studio"
            $appSetupName = "Orion-Studio-$Architecture"
            $appMutex = "OrionStudio-Stable-Instance-Mutex,Orion-Studio-Stable-Instance-Mutex,Zed-Stable-Instance-Mutex"
            $setupMutex = "OrionStudio-Stable-Setup-Mutex"
            $appExeName = "orion-studio"
            $regValueName = "OrionStudio-Stable"
            $appUserId = "OrionStudio-Stable"
            $appShellNameShort = "Or&ion Studio"
            $appxPackageName = "OrionStudio-Stable"
            $applicationRegistrationName = "orion-studio.exe"
            $urlScheme = "orion"
        }
        "preview" {
            $appId = "OrionStudio-Preview"
            $legacyAppId = "{{F70E4811-D0E2-4D88-AC99-D63752799F95}"
            $appIconName = "app-icon-preview"
            $appName = "Orion Studio Preview"
            $appDisplayName = "Orion Studio Preview"
            $appSetupName = "Orion-Studio-$Architecture"
            $appMutex = "OrionStudio-Preview-Instance-Mutex,Orion-Studio-Preview-Instance-Mutex,Zed-Preview-Instance-Mutex"
            $setupMutex = "OrionStudio-Preview-Setup-Mutex"
            $appExeName = "orion-studio"
            $regValueName = "OrionStudio-Preview"
            $appUserId = "OrionStudio-Preview"
            $appShellNameShort = "Or&ion Studio Preview"
            $appxPackageName = "OrionStudio-Preview"
            $applicationRegistrationName = "orion-studio-preview.exe"
            $urlScheme = "orion-preview"
        }
        "nightly" {
            $appId = "OrionStudio-Nightly"
            $legacyAppId = "{{1BDB21D3-14E7-433C-843C-9C97382B2FE0}"
            $appIconName = "app-icon-nightly"
            $appName = "Orion Studio Nightly"
            $appDisplayName = "Orion Studio Nightly"
            $appSetupName = "Orion-Studio-$Architecture"
            $appMutex = "OrionStudio-Nightly-Instance-Mutex,Orion-Studio-Nightly-Instance-Mutex,Zed-Nightly-Instance-Mutex"
            $setupMutex = "OrionStudio-Nightly-Setup-Mutex"
            $appExeName = "orion-studio"
            $regValueName = "OrionStudio-Nightly"
            $appUserId = "OrionStudio-Nightly"
            $appShellNameShort = "Or&ion Studio Nightly"
            $appxPackageName = "OrionStudio-Nightly"
            $applicationRegistrationName = "orion-studio-nightly.exe"
            $urlScheme = "orion-nightly"
        }
        "dev" {
            $appId = "OrionStudio-Dev"
            $legacyAppId = "{{8357632E-24A4-4F32-BA97-E575B4D1FE5D}"
            $appIconName = "app-icon-dev"
            $appName = "Orion Studio Dev"
            $appDisplayName = if ($unsignedDevBuild) { "Orion Studio Dev (Unsigned)" } else { "Orion Studio Dev" }
            $appSetupName = if ($unsignedDevBuild) { "Orion-Studio-Dev-Unsigned-$Architecture" } else { "Orion-Studio-$Architecture" }
            $appMutex = "OrionStudio-Dev-Instance-Mutex,Orion-Studio-Dev-Instance-Mutex,Zed-Dev-Instance-Mutex"
            $setupMutex = "OrionStudio-Dev-Setup-Mutex"
            $appExeName = "orion-studio"
            $regValueName = "OrionStudio-Dev"
            $appUserId = "OrionStudio-Dev"
            $appShellNameShort = "Or&ion Studio Dev"
            $appxPackageName = "OrionStudio-Dev"
            $applicationRegistrationName = "orion-studio-dev.exe"
            $urlScheme = "orion-dev"
        }
        default {
            throw "Cannot bundle an installer for release channel '$channel'."
        }
    }

    $architecturesAllowed = if ($Architecture -eq "aarch64") { "arm64" } else { "x64compatible" }

    # Windows runner 2022 default has iscc in PATH, https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md
    # Currently, we are using Windows 2022 runner.
    # Windows runner 2025 doesn't have iscc in PATH for now, https://github.com/actions/runner-images/issues/11228
    $innoSetupPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"

    $definitions = @{
        "AppId"                = $appId
        "LegacyAppId"          = $legacyAppId
        "AppIconName"          = $appIconName
        "OutputDir"            = "$workspace\target"
        "AppSetupName"         = $appSetupName
        "AppName"              = $appName
        "AppDisplayName"       = $appDisplayName
        "AppPublisher"         = $publisherDisplayName
        "RegValueName"         = $regValueName
        "AppMutex"             = $appMutex
        "SetupMutex"           = $setupMutex
        "AppExeName"           = $appExeName
        "ResourcesDir"         = "$innoDir"
        "ShellNameShort"       = $appShellNameShort
        "AppUserId"            = $appUserId
        "Version"              = "$env:RELEASE_VERSION"
        "SourceDir"            = "$workspace"
        "AppxPackageName"      = $appxPackageName
        "ApplicationRegistrationName" = $applicationRegistrationName
        "UrlScheme"            = $urlScheme
        "ArchitecturesAllowed" = $architecturesAllowed
    }

    $defs = @()
    foreach ($key in $definitions.Keys) {
        $defs += "/d$key=`"$($definitions[$key])`""
    }

    $innoArgs = @($issFilePath) + $defs
    Remove-Item Env:ORION_STUDIO_SIGN_BUNDLE -ErrorAction SilentlyContinue
    if($canCodeSign) {
        $env:ORION_STUDIO_SIGN_BUNDLE = "1"
        $signTool = "powershell.exe -ExecutionPolicy Bypass -File $innoDir\sign.ps1 `$f"
        $innoArgs += "/sDefaultsign=`"$signTool`""
    }

    # Execute Inno Setup
    Write-Host "🚀 Running Inno Setup: $innoSetupPath $innoArgs"
    $process = Start-Process -FilePath $innoSetupPath -ArgumentList $innoArgs -NoNewWindow -Wait -PassThru

    if ($process.ExitCode -eq 0) {
        Write-Host "✅ Inno Setup successfully compiled the installer"
        $script:installerPath = "$workspace\target\$appSetupName.exe"
        if ($canCodeSign) {
            Assert-SignedByConfiguredPublisher -Files @($script:installerPath)
        }
        if ($env:CI) {
            if ([string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
                throw "GITHUB_ENV is required in CI."
            }
            "SETUP_PATH=target/$appSetupName.exe" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
        }
        $script:buildSuccess = $true
    }
    else {
        Write-Host "❌ Inno Setup failed: $($process.ExitCode)"
        $script:buildSuccess = $false
    }
}

ParseOrionStudioWorkspace
CheckEnvironmentVariables
$innoDir = "$workspace\inno\$Architecture"
$debugArchive = "$CargoOutDir\orion-studio-$env:RELEASE_VERSION-$env:ORION_STUDIO_RELEASE_CHANNEL.dbg.zip"

PrepareForBundle
GenerateLicenses
BuildOrionStudioAndTools
BuildRemoteServer
MakeAppx
SignOrionStudioAndTools
ZipOrionStudioDebugSymbols
DownloadAMDGpuServices
DownloadConpty
CollectFiles
BuildInstaller

if($env:CI) {
    UploadToSentry
}

if ($buildSuccess) {
    Write-Output "Build successful"
    if ($Install) {
        Write-Output "Installing Orion Studio..."
        Start-Process -FilePath $installerPath
    }
    exit 0
}
else {
    Write-Output "Build failed"
    exit 1
}
