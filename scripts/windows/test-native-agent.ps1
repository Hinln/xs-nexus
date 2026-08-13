[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$EvidenceDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$secretScanner = Join-Path $root 'scripts\check-secrets.py'
$evidence = [System.IO.Path]::GetFullPath((Join-Path $root $EvidenceDirectory))
if (-not $evidence.StartsWith($root + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'evidence directory must remain inside the repository checkout'
}

New-Item -ItemType Directory -Path $evidence -Force | Out-Null
$revision = (& git -C $root rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $revision -notmatch '^[0-9a-f]{40}$') {
    throw 'unable to resolve the exact Git revision'
}
Set-Content -LiteralPath (Join-Path $evidence 'revision.txt') -Value $revision -Encoding utf8NoBOM

$rustVersion = (& rustc -vV) -join "`n"
if ($LASTEXITCODE -ne 0 -or $rustVersion -notmatch '(?m)^host: x86_64-pc-windows-msvc$') {
    throw 'native Windows validation requires the x86_64-pc-windows-msvc host toolchain'
}
$cargoVersion = (& cargo -V).Trim()
$environment = @(
    "revision=$revision"
    "os=$([System.Environment]::OSVersion.VersionString)"
    "process_architecture=$([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture)"
    $rustVersion
    $cargoVersion
)
Set-Content -LiteralPath (Join-Path $evidence 'environment.txt') -Value $environment -Encoding utf8NoBOM

function Invoke-LoggedCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$LogName,
        [Parameter(Mandatory = $true)]
        [string]$Command,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    & $Command @Arguments 2>&1 | Tee-Object -FilePath (Join-Path $evidence $LogName)
    if ($LASTEXITCODE -ne 0) {
        throw "$Command failed while producing $LogName"
    }
}

$packages = @(
    'xs-agent',
    'xs-cli',
    'xs-windows-local-ipc',
    'xs-windows-private-storage',
    'xs-windows-route-manager',
    'xs-windows-service',
    'xs-windows-transport',
    'xs-windows-wintun'
)
$packageArguments = foreach ($package in $packages) { '-p'; $package }
$status = 'FAIL'

try {
    $trackedChanges = (& git -C $root status --porcelain --untracked-files=no) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $trackedChanges) {
        throw 'native Windows validation requires a clean tracked checkout'
    }

    Push-Location $root
    try {
        Invoke-LoggedCommand -LogName 'build.log' -Command 'cargo' -Arguments (@('build', '--locked', '--release') + $packageArguments)
        Invoke-LoggedCommand -LogName 'agent-tests.log' -Command 'cargo' -Arguments @('test', '--locked', '-p', 'xs-agent', '--lib')
        Invoke-LoggedCommand -LogName 'cli-tests.log' -Command 'cargo' -Arguments @('test', '--locked', '-p', 'xs-cli')
        Invoke-LoggedCommand -LogName 'windows-boundary-tests.log' -Command 'cargo' -Arguments (@('test', '--locked') + $packageArguments[4..($packageArguments.Count - 1)])
        Invoke-LoggedCommand -LogName 'clippy.log' -Command 'cargo' -Arguments (@('clippy', '--locked', '--all-targets') + $packageArguments + @('--', '-D', 'warnings'))
    }
    finally {
        Pop-Location
    }

    $binaryHashes = @(
        Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $root 'target\release\xs-agent.exe')
        Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $root 'target\release\xs.exe')
    ) | ForEach-Object {
        "$($_.Hash.ToLowerInvariant())  $([System.IO.Path]::GetRelativePath($root, $_.Path).Replace('\', '/'))"
    }
    Set-Content -LiteralPath (Join-Path $evidence 'binary-sha256.txt') -Value $binaryHashes -Encoding utf8NoBOM

    $sourceScan = & python $secretScanner --root $root 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw 'repository secret scan failed'
    }
    Set-Content -LiteralPath (Join-Path $evidence 'source-secret-scan.log') -Value $sourceScan -Encoding utf8NoBOM

    $evidenceScan = & python scripts/check-secrets.py --root $evidence 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw 'evidence secret scan failed'
    }
    Set-Content -LiteralPath (Join-Path $evidence 'secret-scan.log') -Value $evidenceScan -Encoding utf8NoBOM
    $status = 'PASS'
}
catch {
    Set-Content -LiteralPath (Join-Path $evidence 'error.txt') -Value $_.Exception.Message -Encoding utf8NoBOM
    throw
}
finally {
    Set-Content -LiteralPath (Join-Path $evidence 'status.txt') -Value $status -Encoding utf8NoBOM
    Set-Content -LiteralPath (Join-Path $evidence 'summary.txt') -Value @(
        "revision=$revision"
        'host=x86_64-pc-windows-msvc'
        "status=$status"
        'device_installation=false'
        'driver_verifier=false'
        'production_mutation=false'
    ) -Encoding utf8NoBOM

    $manifest = Get-ChildItem -LiteralPath $evidence -Recurse -File |
        Where-Object Name -ne 'SHA256SUMS' |
        Sort-Object FullName |
        ForEach-Object {
            $relative = [System.IO.Path]::GetRelativePath($evidence, $_.FullName).Replace('\', '/')
            "$((Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant())  ./$relative"
        }
    Set-Content -LiteralPath (Join-Path $evidence 'SHA256SUMS') -Value $manifest -Encoding utf8NoBOM
}

Write-Output "native Windows Agent evidence passed at revision $revision"
