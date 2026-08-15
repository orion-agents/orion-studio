
function ParseOrionStudioWorkspace {
    $metadata = cargo metadata --no-deps --offline | ConvertFrom-Json
    $env:ORION_STUDIO_WORKSPACE = $metadata.workspace_root
    $env:ZED_WORKSPACE = $metadata.workspace_root
    $env:RELEASE_VERSION = $metadata.packages | Where-Object { $_.name -eq "orion-studio" } | Select-Object -ExpandProperty version
    if ([string]::IsNullOrWhiteSpace($env:RELEASE_VERSION)) {
        throw "Unable to resolve the orion-studio package version."
    }
}

function ParseZedWorkspace {
    ParseOrionStudioWorkspace
}
