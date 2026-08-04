[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string] $SourceRoot,
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string] $OutputDirectory,
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string] $WintunArchive,
    [switch] $SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Version = '0.1.0'
$Target = 'x86_64-pc-windows-msvc'
$Architecture = 'x86_64'
$WintunVersion = '0.14.1'
$ExpectedWintunArchiveSha256 = '07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51'
$ExpectedWintunDllSha256 = 'e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce'

function Fail([string] $Message) {
    throw "XS Nexus Windows release build failed: $Message"
}

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) {
        Fail $Message
    }
}

function Resolve-RegularFile([string] $Path, [string] $Name) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    Assert-True (-not $item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) "$Name must be a regular non-reparse file"
    return $item.FullName
}

function Get-Sha256([string] $Path) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Write-Utf8NoBom([string] $Path, [string] $Value) {
    [IO.File]::WriteAllText($Path, $Value, [Text.UTF8Encoding]::new($false))
}

$SourceRoot = (Get-Item -LiteralPath $SourceRoot -Force -ErrorAction Stop).FullName
Assert-True (Test-Path -LiteralPath (Join-Path $SourceRoot 'Cargo.toml') -PathType Leaf) 'SourceRoot is not an XS Nexus checkout'
$WintunArchive = Resolve-RegularFile $WintunArchive 'Wintun archive'
Assert-True ((Get-Sha256 $WintunArchive) -eq $ExpectedWintunArchiveSha256) 'Wintun archive hash does not match the approved release'
Assert-True (-not (Test-Path -LiteralPath $OutputDirectory)) 'OutputDirectory must not already exist'

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("xs-nexus-windows-release-" + [Guid]::NewGuid().ToString('N'))
try {
    if (-not $SkipBuild) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        Assert-True (Test-Path -LiteralPath $vswhere -PathType Leaf) 'Visual Studio Build Tools are unavailable'
        $vsRoot = (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
        Assert-True (-not [string]::IsNullOrWhiteSpace($vsRoot)) 'x64 C++ Build Tools are unavailable'
        $developerCommand = Join-Path $vsRoot 'Common7\Tools\VsDevCmd.bat'
        Assert-True (Test-Path -LiteralPath $developerCommand -PathType Leaf) 'Visual Studio developer command is unavailable'
        $cargo = (Get-Command cargo -ErrorAction Stop).Source
        $build = 'call "{0}" -arch=x64 -host_arch=x64 && "{1}" build --manifest-path "{2}" --locked --release --target {3} -p xs-agent -p xs-cli' -f $developerCommand, $cargo, (Join-Path $SourceRoot 'Cargo.toml'), $Target
        & cmd.exe /d /c $build
        Assert-True ($LASTEXITCODE -eq 0) 'Cargo release build failed'
    }

    $agent = Resolve-RegularFile (Join-Path $SourceRoot "target\$Target\release\xs-agent.exe") 'xs-agent.exe'
    $cli = Resolve-RegularFile (Join-Path $SourceRoot "target\$Target\release\xs.exe") 'xs.exe'
    New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
    $wintunExtract = Join-Path $temporaryRoot 'wintun'
    Expand-Archive -LiteralPath $WintunArchive -DestinationPath $wintunExtract -ErrorAction Stop
    $wintunDll = Resolve-RegularFile (Join-Path $wintunExtract 'wintun\bin\amd64\wintun.dll') 'wintun.dll'
    $wintunLicense = Resolve-RegularFile (Join-Path $wintunExtract 'wintun\LICENSE.txt') 'Wintun license'
    Assert-True ((Get-Sha256 $wintunDll) -eq $ExpectedWintunDllSha256) 'Wintun DLL hash does not match the approved release'
    $signature = Get-AuthenticodeSignature -LiteralPath $wintunDll
    Assert-True ($signature.Status -eq 'Valid') 'Wintun DLL code signature is invalid'
    Assert-True ($signature.SignerCertificate.Subject -match '(^|,\s*)CN=WireGuard LLC(?:,|$)') 'Wintun DLL signer identity is invalid'

    New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
    $stage = Join-Path $temporaryRoot 'payload'
    $thirdParty = Join-Path $stage 'THIRD_PARTY'
    New-Item -ItemType Directory -Path $stage, $thirdParty | Out-Null
    Copy-Item -LiteralPath $agent -Destination (Join-Path $stage 'xs-agent.exe')
    Copy-Item -LiteralPath $cli -Destination (Join-Path $stage 'xs.exe')
    Copy-Item -LiteralPath $wintunDll -Destination (Join-Path $stage 'wintun.dll')
    Copy-Item -LiteralPath $wintunLicense -Destination (Join-Path $thirdParty 'WINTUN-LICENSE.txt')
    $payloadFiles = @('xs-agent.exe', 'xs.exe', 'wintun.dll', 'THIRD_PARTY\WINTUN-LICENSE.txt')
    $payloadLines = foreach ($relativePath in $payloadFiles) {
        '{0}  {1}' -f (Get-Sha256 (Join-Path $stage $relativePath)), $relativePath
    }
    Write-Utf8NoBom (Join-Path $stage 'PAYLOAD.SHA256') (($payloadLines -join "`n") + "`n")

    $archiveName = "xs-nexus-$Version-$Target.zip"
    $archivePath = Join-Path $OutputDirectory $archiveName
    Compress-Archive -LiteralPath (Get-ChildItem -LiteralPath $stage -Force | Select-Object -ExpandProperty FullName) -DestinationPath $archivePath -CompressionLevel Optimal
    $manifest = [ordered]@{
        schema_version = 1
        product = 'xs-nexus'
        version = $Version
        platform = 'windows'
        architecture = $Architecture
        target = $Target
        archive = $archiveName
        archive_size = (Get-Item -LiteralPath $archivePath).Length
        archive_sha256 = Get-Sha256 $archivePath
        wintun = [ordered]@{
            version = $WintunVersion
            sha256 = $ExpectedWintunDllSha256
        }
    }
    $manifestName = "xs-nexus-$Version-$Target.manifest.json"
    $manifestPath = Join-Path $OutputDirectory $manifestName
    Write-Utf8NoBom $manifestPath (($manifest | ConvertTo-Json -Depth 4 -Compress) + "`n")
    $manifestSha256 = Get-Sha256 $manifestPath
    $bootstrapTemplate = Get-Content -LiteralPath (Join-Path $SourceRoot 'installers\windows\xs-nexus-one-click.ps1') -Raw -Encoding UTF8
    Assert-True (([regex]::Matches($bootstrapTemplate, [regex]::Escape('__RELEASE_MANIFEST_SHA256__'))).Count -eq 1) 'bootstrap template marker is invalid'
    $bootstrap = $bootstrapTemplate.Replace('__RELEASE_MANIFEST_SHA256__', $manifestSha256)
    Write-Utf8NoBom (Join-Path $OutputDirectory 'install.ps1') $bootstrap

    Get-ChildItem -LiteralPath $OutputDirectory -File | Sort-Object Name | Select-Object Name, Length, LastWriteTime
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
