[CmdletBinding()]
Param(
    [Parameter(Mandatory = $true)][ValidateSet("x86_64", "aarch64")][string]$Architecture
)

# Maintenance-only manual uploader. Active generated workflows publish through
# script/upload-nightly; keep this script for explicit Windows recovery runs.
# Based on the template in: https://docs.digitalocean.com/reference/api/spaces-api/
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\lib\blob-store.ps1"
. "$PSScriptRoot\lib\workspace.ps1"

ParseOrionStudioWorkspace
Write-Host "Uploading Orion Studio nightly for architecture: $Architecture"

$bucketName = $env:ORION_STUDIO_NIGHTLY_BUCKET
if ([string]::IsNullOrWhiteSpace($bucketName)) {
    throw "ORION_STUDIO_NIGHTLY_BUCKET is required."
}
foreach ($credentialName in @("DIGITALOCEAN_SPACES_ACCESS_KEY", "DIGITALOCEAN_SPACES_SECRET_KEY")) {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($credentialName))) {
        throw "$credentialName is required for a nightly upload."
    }
}
if ([string]::IsNullOrWhiteSpace($env:GITHUB_RUN_NUMBER) -or [string]::IsNullOrWhiteSpace($env:GITHUB_SHA)) {
    throw "GITHUB_RUN_NUMBER and GITHUB_SHA are required for a nightly upload."
}

$releaseVersion = & "$PSScriptRoot\get-crate-version.ps1" orion-studio
if (-not $? -or [string]::IsNullOrWhiteSpace($releaseVersion)) {
    throw "Unable to resolve the orion-studio release version."
}
$version = "$releaseVersion+nightly.$env:GITHUB_RUN_NUMBER.$env:GITHUB_SHA"

$targetDirectory = Join-Path $env:ORION_STUDIO_WORKSPACE "target"
$remoteServerName = "orion-studio-remote-server-windows-$Architecture.zip"
$remoteServerFiles = @(Get-ChildItem -LiteralPath $targetDirectory -Filter $remoteServerName -File)
if ($remoteServerFiles.Count -ne 1) {
    throw "Expected exactly one '$remoteServerName' in '$targetDirectory', found $($remoteServerFiles.Count)."
}
$remoteServerFile = $remoteServerFiles[0]
UploadToBlobStore -BucketName $bucketName -FileToUpload $remoteServerFile.FullName -BlobStoreKey "nightly/$($remoteServerFile.Name)"
UploadToBlobStore -BucketName $bucketName -FileToUpload $remoteServerFile.FullName -BlobStoreKey "$version/$($remoteServerFile.Name)"
Remove-Item -LiteralPath $remoteServerFile.FullName -Force

$installerPath = Join-Path $targetDirectory "Orion-Studio-$Architecture.exe"
if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    throw "Expected installer '$installerPath' was not found."
}
UploadToBlobStore -BucketName $bucketName -FileToUpload $installerPath -BlobStoreKey "nightly/Orion-Studio-$Architecture.exe"
UploadToBlobStore -BucketName $bucketName -FileToUpload $installerPath -BlobStoreKey "$version/Orion-Studio-$Architecture.exe"

Remove-Item -LiteralPath $installerPath -Force

$latestShaPath = Join-Path $targetDirectory "latest-sha"
$version | Out-File -FilePath $latestShaPath -NoNewline
try {
    UploadToBlobStore -BucketName $bucketName -FileToUpload $latestShaPath -BlobStoreKey "nightly/latest-sha-windows"
} finally {
    if (Test-Path -LiteralPath $latestShaPath) {
        Remove-Item -LiteralPath $latestShaPath -Force
    }
}
