#Requires -Version 7.4
#Requires -RunAsAdministrator

[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [Parameter(Mandatory)][string]$PackageDirectory,
    [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{40,128}$')]
    [string]$ExpectedSignerThumbprint,
    [Parameter(Mandatory)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$')]
    [string]$ExpectedDriverVersion,
    [Parameter(Mandatory)][string]$DevGenPath,
    [switch]$AllowTestSignedPackage
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'XsnetTestInstaller.psm1') -Force

if (-not $AllowTestSignedPackage) {
    throw 'test-signed driver installation requires -AllowTestSignedPackage'
}
Assert-XsnetTestHost
$packageItem = Get-Item -LiteralPath $PackageDirectory -ErrorAction Stop
if (-not $packageItem.PSIsContainer -or
    ($packageItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'driver package directory must be a real directory, not a reparse point'
}
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
$expectedFiles = @('xsnet.cat', 'xsnet.dll', 'xsnet.inf')
$entries = @(Get-ChildItem -LiteralPath $package -Force)
if ($entries | Where-Object {
    $_.PSIsContainer -or ($_.Attributes -band [IO.FileAttributes]::ReparsePoint)
}) {
    throw 'driver package cannot contain directories or reparse points'
}
$actualFiles = @($entries | ForEach-Object Name | Sort-Object)
if (Compare-Object $expectedFiles $actualFiles) {
    throw 'driver package must contain exactly xsnet.inf, xsnet.cat, and xsnet.dll'
}
$thumbprint = ($ExpectedSignerThumbprint -replace '[^0-9A-Fa-f]', '').ToUpperInvariant()
$driverVersion = Get-XsnetInfDriverVersion -Path (Join-Path $package 'xsnet.inf')
if ($driverVersion -cne $ExpectedDriverVersion) {
    throw 'xsnet INF DriverVer does not match the expected package version'
}
$abiVersion = Get-XsnetAbiVersion
Assert-XsnetSignature -Path (Join-Path $package 'xsnet.cat') -ExpectedThumbprint $thumbprint
Assert-XsnetSignature -Path (Join-Path $package 'xsnet.dll') -ExpectedThumbprint $thumbprint
$devgen = Assert-XsnetDevGen -Path $DevGenPath
if (@(Get-XsnetDevices).Count -ne 0 -or
    @(Get-XsnetDriverPackages).Count -ne 0) {
    throw 'an xsnet device or driver package already exists; uninstall it first'
}
if (-not $PSCmdlet.ShouldProcess('ROOT\XSNET', 'Install test-signed xsnet driver')) {
    return
}

$publishedInf = $null
try {
    Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\pnputil.exe" -Arguments @(
        '/add-driver',
        (Join-Path $package 'xsnet.inf')
    ) | Out-Null
    $packages = @(Get-XsnetDriverPackages)
    if ($packages.Count -ne 1 -or $packages[0].Driver -notmatch '^oem[0-9]+\.inf$' -or
        $packages[0].Version.ToString() -cne $driverVersion) {
        throw 'unable to identify the staged xsnet driver package'
    }
    $publishedInf = $packages[0].Driver
    Invoke-XsnetNative -FilePath $devgen -Arguments @(
        '/add',
        '/bus',
        'ROOT',
        '/instanceid',
        'XSNET',
        '/hardwareid',
        'Root\XSNET'
    ) | Out-Null
    # DevGen creates the deterministic root device but does not select a
    # staged driver. Re-run PnPUtil with /install after enumeration so Windows
    # binds only the exact signed package that was already validated above.
    Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\pnputil.exe" -Arguments @(
        '/add-driver',
        (Join-Path $package 'xsnet.inf'),
        '/install'
    ) | Out-Null
    $devices = @()
    for ($attempt = 0; $attempt -lt 40; $attempt += 1) {
        $devices = @(Get-XsnetDevices)
        if ($devices.Count -eq 1 -and $devices[0].Status -eq 'OK' -and
            $devices[0].Class -eq 'Net') {
            break
        }
        Start-Sleep -Milliseconds 500
    }
    if ($devices.Count -ne 1 -or $devices[0].Status -ne 'OK' -or
        $devices[0].Class -ne 'Net') {
        $diagnostic = @($devices | Select-Object Status, Class, FriendlyName,
            InstanceId, Problem, Present) | ConvertTo-Json -Compress
        Write-Output "xsnet device health diagnostic: $diagnostic"
        throw 'xsnet device did not reach a healthy Net-class state'
    }
    $state = [ordered]@{
        schema = 2
        abi_version = $abiVersion
        driver_version = $driverVersion
        hardware_id = 'Root\XSNET'
        published_inf = $publishedInf
        device_instance_ids = @($devices | ForEach-Object InstanceId)
        signer_thumbprint = $thumbprint
        package_hashes = [ordered]@{
            cat = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $package 'xsnet.cat')).Hash
            dll = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $package 'xsnet.dll')).Hash
            inf = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $package 'xsnet.inf')).Hash
        }
        installed_at_utc = [DateTime]::UtcNow.ToString('O')
        test_only = $true
    }
    Write-XsnetInstallState -State $state
} catch {
    foreach ($device in @(Get-XsnetDevices)) {
        Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\pnputil.exe" -Arguments @(
            '/remove-device',
            $device.InstanceId,
            '/subtree'
        ) | Out-Null
    }
    if ($null -ne $publishedInf) {
        Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\pnputil.exe" -Arguments @(
            '/delete-driver',
            $publishedInf,
            '/uninstall',
            '/force'
        ) | Out-Null
    }
    throw
}
