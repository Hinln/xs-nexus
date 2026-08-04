#Requires -Version 7.4
#Requires -RunAsAdministrator

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ProjectRoot,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$MsBuildPath,
    [Parameter(Mandatory)][string]$InfVerifPath,
    [Parameter(Mandatory)][string]$Inf2CatPath,
    [Parameter(Mandatory)][string]$SignToolPath,
    [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{40,128}$')]
    [string]$TestSignerThumbprint,
    [switch]$CertificateInLocalMachineStore,
    [switch]$ConfirmDisposableVm,
    [switch]$AllowTestSigning
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:ApprovedInf2CatSha256 = `
    'B594728D38B271979367ABC8060A971B8E42422738009BE126710B1F5DD0FCBC'

if (-not $ConfirmDisposableVm -or -not $AllowTestSigning) {
    throw 'test package generation requires -ConfirmDisposableVm and -AllowTestSigning'
}
if ([Environment]::OSVersion.Version.Build -lt 26100) {
    throw 'Windows build 26100 or newer is required'
}

function Resolve-RealPath {
    param([Parameter(Mandatory)][string]$Path, [switch]$Directory)

    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ([bool]$item.PSIsContainer -ne [bool]$Directory) {
        throw "unexpected path type: $Path"
    }
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "reparse points are not allowed: $Path"
    }
    return $item.FullName
}

function Resolve-MicrosoftTool {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = Resolve-RealPath -Path $Path
    $signature = Get-AuthenticodeSignature -LiteralPath $resolved
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Subject -notmatch '(?i)Microsoft') {
        throw "tool is not validly Microsoft-signed: $Path"
    }
    return $resolved
}

function Assert-Amd64MsBuildPath {
    param([Parameter(Mandatory)][string]$Path)

    if ($Path -notmatch '(?i)\\MSBuild\\Current\\Bin\\amd64\\MSBuild\.exe$') {
        throw 'WDK 26100 packaging requires the amd64 MSBuild executable'
    }
}

function Resolve-ApprovedInf2Cat {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = Resolve-RealPath -Path $Path
    $signature = Get-AuthenticodeSignature -LiteralPath $resolved
    if ($signature.Status -eq 'Valid' -and $null -ne $signature.SignerCertificate -and
        $signature.SignerCertificate.Subject -match '(?i)Microsoft') {
        return $resolved
    }

    $file = Get-Item -LiteralPath $resolved
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $resolved).Hash
    $isPinnedWdkTool =
        $resolved -match '(?i)\\Windows Kits\\10\\bin\\10\.0\.26100\.0\\x86\\Inf2Cat\.exe$' -and
        $hash -ceq $script:ApprovedInf2CatSha256 -and
        $file.VersionInfo.FileDescription -ceq 'Inf2Cat' -and
        $file.VersionInfo.ProductVersion -ceq '1.0.0519.24+3dc05997' -and
        $null -ne $signature.SignerCertificate -and
        $signature.SignerCertificate.Subject -ceq `
            'CN=Windows Internal Build Tools CodeSign, O=Microsoft Corporation, L=Redmond, S=Washington, C=US' -and
        $signature.SignerCertificate.Thumbprint -ceq `
            '0061B7902128989780FAB7D913EA8D60FF9BB766' -and
        $null -ne $signature.TimeStamperCertificate -and
        $signature.TimeStamperCertificate.Subject -match '(?i)Microsoft Time-Stamp Service'
    if (-not $isPinnedWdkTool) {
        throw "Inf2Cat is neither validly Microsoft-signed nor the pinned WDK 26100.6584 binary: $Path"
    }
    return $resolved
}

function Invoke-NativeTool {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$LogPath
    )

    & $Path @Arguments 2>&1 | Tee-Object -LiteralPath $LogPath
    if ($LASTEXITCODE -ne 0) {
        throw "native tool failed with exit code ${LASTEXITCODE}: $Path"
    }
}

$root = Resolve-RealPath -Path $ProjectRoot -Directory
$driverRoot = Resolve-RealPath -Path (Join-Path $root 'drivers\windows-xsnet') -Directory
$project = Resolve-RealPath -Path (Join-Path $driverRoot 'xsnet.vcxproj')
if (-not [IO.Path]::IsPathFullyQualified($OutputDirectory)) {
    throw 'output directory must be an absolute path'
}
$outputParent = Split-Path -Parent $OutputDirectory
if (-not (Test-Path -LiteralPath $outputParent -PathType Container)) {
    throw 'output parent directory must already exist'
}
$outputParent = Resolve-RealPath -Path $outputParent -Directory
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not $output.StartsWith($outputParent + [IO.Path]::DirectorySeparatorChar,
        [StringComparison]::OrdinalIgnoreCase)) {
    throw 'output directory must be a new child of its existing parent'
}
if (Test-Path -LiteralPath $output) {
    throw 'output directory must not already exist'
}

$msbuild = Resolve-MicrosoftTool -Path $MsBuildPath
Assert-Amd64MsBuildPath -Path $msbuild
$infverif = Resolve-MicrosoftTool -Path $InfVerifPath
$inf2cat = Resolve-ApprovedInf2Cat -Path $Inf2CatPath
$signtool = Resolve-MicrosoftTool -Path $SignToolPath
$thumbprint = ($TestSignerThumbprint -replace '[^0-9A-Fa-f]', '').ToUpperInvariant()
$storeLocation = if ($CertificateInLocalMachineStore) { 'LocalMachine' } else { 'CurrentUser' }
$certificate = Get-Item -LiteralPath "Cert:\$storeLocation\My\$thumbprint" -ErrorAction Stop
if (-not $certificate.HasPrivateKey -or $certificate.NotBefore.ToUniversalTime() -gt [DateTime]::UtcNow -or
    $certificate.NotAfter.ToUniversalTime() -le [DateTime]::UtcNow) {
    throw 'test signer certificate is not currently valid with a private key'
}
$certificateEkus = @(
    $certificate.EnhancedKeyUsageList | ForEach-Object { [string]$_.ObjectId }
)
if (-not ($certificateEkus -contains '1.3.6.1.5.5.7.3.3')) {
    throw 'test signer certificate lacks the code-signing EKU'
}

$null = New-Item -ItemType Directory -Path $output
$buildOutput = New-Item -ItemType Directory -Path (Join-Path $output 'build')
$intermediate = New-Item -ItemType Directory -Path (Join-Path $output 'intermediate')
$package = New-Item -ItemType Directory -Path (Join-Path $output 'package')
$logs = New-Item -ItemType Directory -Path (Join-Path $output 'logs')

Invoke-NativeTool -Path $msbuild -Arguments @(
    $project,
    '/t:Rebuild',
    '/m:1',
    '/p:Configuration=Release',
    '/p:Platform=x64',
    '/p:SignMode=Off',
    '/p:EnableInf2Cat=false',
    "/p:OutDir=$($buildOutput.FullName)\",
    "/p:IntDir=$($intermediate.FullName)\"
) -LogPath (Join-Path $logs 'msbuild.log')

$builtDll = Resolve-RealPath -Path (Join-Path $buildOutput 'xsnet.dll')
$builtInf = Resolve-RealPath -Path (Join-Path $buildOutput 'xsnet.inf')
$driverVersionLines = @(Get-Content -LiteralPath $builtInf | Where-Object {
    $_ -match '^\s*DriverVer\s*=\s*[^,]+,\s*(?<Version>[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)\s*$'
})
if ($driverVersionLines.Count -ne 1) {
    throw 'built INF must contain exactly one four-part DriverVer'
}
$driverVersion = [regex]::Match(
    $driverVersionLines[0],
    '^\s*DriverVer\s*=\s*[^,]+,\s*(?<Version>[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)\s*$'
).Groups['Version'].Value
Copy-Item -LiteralPath $builtDll -Destination (Join-Path $package 'xsnet.dll')
Copy-Item -LiteralPath $builtInf -Destination (Join-Path $package 'xsnet.inf')

Invoke-NativeTool -Path $infverif -Arguments @(
    '/w',
    '/v',
    (Join-Path $package 'xsnet.inf')
) -LogPath (Join-Path $logs 'infverif.log')

$signArguments = @('sign', '/v', '/fd', 'SHA256', '/sha1', $thumbprint, '/s', 'My')
if ($CertificateInLocalMachineStore) {
    $signArguments += '/sm'
}
Invoke-NativeTool -Path $signtool -Arguments ($signArguments + (Join-Path $package 'xsnet.dll')) `
    -LogPath (Join-Path $logs 'sign-xsnet.dll.log')

Invoke-NativeTool -Path $inf2cat -Arguments @(
    "/driver:$($package.FullName)",
    '/os:10_GE_X64',
    '/verbose'
) -LogPath (Join-Path $logs 'inf2cat.log')

$catalog = Resolve-RealPath -Path (Join-Path $package 'xsnet.cat')
Invoke-NativeTool -Path $signtool -Arguments ($signArguments + $catalog) `
    -LogPath (Join-Path $logs 'sign-xsnet.cat.log')

$expectedFiles = @('xsnet.cat', 'xsnet.dll', 'xsnet.inf')
$actualFiles = @(Get-ChildItem -LiteralPath $package -Force | ForEach-Object Name | Sort-Object)
if (Compare-Object $expectedFiles $actualFiles) {
    throw 'test package must contain exactly xsnet.inf, xsnet.cat, and xsnet.dll'
}
foreach ($file in @('xsnet.cat', 'xsnet.dll')) {
    $signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $package $file)
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint -ne $thumbprint) {
        throw "signed package file failed thumbprint validation: $file"
    }
}

$manifest = [ordered]@{
    schema = 1
    test_only = $true
    target = 'Windows 11 24H2 x64'
    os_identifier = '10_GE_X64'
    driver_version = $driverVersion
    abi_min = 1
    abi_max = 1
    capabilities = @('ipv4')
    signer_thumbprint = $thumbprint
    generated_at_utc = [DateTime]::UtcNow.ToString('O')
    files = [ordered]@{}
}
foreach ($file in $expectedFiles) {
    $manifest.files[$file] = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $package $file)).Hash
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output 'manifest.json') `
    -Encoding utf8NoBOM
Write-Output $package.FullName
