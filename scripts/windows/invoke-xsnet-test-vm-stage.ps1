#Requires -Version 7.4
#Requires -RunAsAdministrator

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Initialize', 'Install', 'EnableVerifier', 'CollectVerifier',
        'DisableVerifier', 'Uninstall')]
    [string]$Stage,
    [Parameter(Mandatory)][string]$RunDirectory,
    [ValidateSet('VirtualMachine', 'PhysicalMachine')]
    [string]$TargetType = 'VirtualMachine',
    [ValidateLength(1, 256)][string]$SnapshotId,
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$')]
    [string]$SystemImageId,
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$')]
    [string]$RecoveryMediaId,
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$')]
    [string]$DiskRecoveryReceiptId,
    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$')]
    [string]$RecoveryOperatorId,
    [string]$PackageDirectory,
    [ValidatePattern('^[0-9A-Fa-f]{40,128}$')]
    [string]$ExpectedSignerThumbprint,
    [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$')]
    [string]$ExpectedDriverVersion,
    [string]$DevGenPath,
    [switch]$ConfirmDisposableVm,
    [switch]$ConfirmSnapshotAvailable,
    [switch]$ConfirmDedicatedPhysicalTarget,
    [switch]$ConfirmExternalSystemImageAvailable,
    [switch]$ConfirmBootableRecoveryMediaAvailable,
    [switch]$ConfirmDiskRecoveryMaterialAvailable,
    [switch]$ConfirmOnsiteRecoveryOperatorAvailable
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($TargetType -eq 'VirtualMachine') {
    if (-not $ConfirmDisposableVm -or -not $ConfirmSnapshotAvailable -or
        [string]::IsNullOrWhiteSpace($SnapshotId)) {
        throw 'virtual-machine validation requires disposable-VM, snapshot, and snapshot-ID confirmations'
    }
    $recoveryIdentifier = $SnapshotId
} else {
    $physicalIdentifiers = @(
        $SystemImageId,
        $RecoveryMediaId,
        $DiskRecoveryReceiptId,
        $RecoveryOperatorId
    )
    if (-not $ConfirmDedicatedPhysicalTarget -or
        -not $ConfirmExternalSystemImageAvailable -or
        -not $ConfirmBootableRecoveryMediaAvailable -or
        -not $ConfirmDiskRecoveryMaterialAvailable -or
        -not $ConfirmOnsiteRecoveryOperatorAvailable -or
        $physicalIdentifiers.Where({ [string]::IsNullOrWhiteSpace($_) }).Count -ne 0) {
        throw 'physical-machine validation requires dedicated-target, external-image, bootable-media, disk-recovery, and onsite-recovery confirmations'
    }
    $recoveryIdentifier = $SystemImageId
}
if ([Environment]::OSVersion.Version.Build -lt 26100) {
    throw 'Windows build 26100 or newer is required'
}
if (-not [IO.Path]::IsPathFullyQualified($RunDirectory)) {
    throw 'run directory must be an absolute path'
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

function Assert-ApprovedTestTarget {
    $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
    $virtualIdentity = "$($computer.Manufacturer) $($computer.Model)"
    $knownVirtualIdentity = $virtualIdentity -match
        '(?i)virtual|vmware|virtualbox|kvm|qemu|xen|hyper-v|parallels'
    if ($TargetType -eq 'VirtualMachine' -and -not $knownVirtualIdentity) {
        throw 'refusing to run because the target is not identified as a virtual machine'
    }
    if ($TargetType -eq 'PhysicalMachine' -and $knownVirtualIdentity) {
        throw 'refusing physical-machine mode because the target is identified as virtual'
    }
    return [ordered]@{
        computer_name = $env:COMPUTERNAME
        manufacturer = [string]$computer.Manufacturer
        model = [string]$computer.Model
        hypervisor_present = [bool]$computer.HypervisorPresent
        target_type = $TargetType
        virtual_identity_detected = [bool]$knownVirtualIdentity
    }
}

function Get-DiskProtectionEvidence {
    $systemDrive = $env:SystemDrive
    if ($null -eq (Get-Command Get-BitLockerVolume -ErrorAction SilentlyContinue)) {
        return [ordered]@{
            mount_point = $systemDrive
            status = 'cmdlet-unavailable'
        }
    }
    $volume = Get-BitLockerVolume -MountPoint $systemDrive -ErrorAction Stop
    return [ordered]@{
        mount_point = [string]$volume.MountPoint
        volume_status = [string]$volume.VolumeStatus
        protection_status = [string]$volume.ProtectionStatus
        encryption_method = [string]$volume.EncryptionMethod
        encryption_percentage = [int]$volume.EncryptionPercentage
    }
}

function Invoke-NativeTool {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string[]]$Arguments,
        [Parameter(Mandatory)][string]$LogPath,
        [int[]]$AcceptedExitCodes = @(0)
    )

    & $Path @Arguments 2>&1 | Tee-Object -LiteralPath $LogPath
    if ($AcceptedExitCodes -notcontains $LASTEXITCODE) {
        throw "native tool failed with exit code ${LASTEXITCODE}: $Path"
    }
}

function New-StageDirectory {
    param([Parameter(Mandatory)][string]$Name)

    $path = Join-Path $script:RunRoot $Name
    if (Test-Path -LiteralPath $path) {
        throw "stage evidence already exists: $Name"
    }
    return New-Item -ItemType Directory -Path $path
}

function Read-RunManifest {
    $manifestPath = Join-Path $script:RunRoot 'run.json'
    $manifestItem = Get-Item -LiteralPath $manifestPath -Force -ErrorAction Stop
    if ($manifestItem.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw 'run manifest cannot be a reparse point'
    }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.schema -ne 2 -or $manifest.target_type -cne $TargetType -or
        $manifest.recovery_identifier -cne $recoveryIdentifier -or
        $manifest.computer_name -cne $env:COMPUTERNAME) {
        throw 'run manifest does not match this target and recovery assertion'
    }
    if ($TargetType -eq 'PhysicalMachine' -and
        ($manifest.physical_recovery.system_image_id -cne $SystemImageId -or
         $manifest.physical_recovery.recovery_media_id -cne $RecoveryMediaId -or
         $manifest.physical_recovery.disk_recovery_receipt_id -cne $DiskRecoveryReceiptId -or
         $manifest.physical_recovery.recovery_operator_id -cne $RecoveryOperatorId)) {
        throw 'run manifest does not match the physical recovery material identifiers'
    }
    return $manifest
}

function Assert-CompletedStage {
    param([Parameter(Mandatory)][string]$Name)

    $marker = Join-Path $script:RunRoot "$Name\completed.json"
    $markerItem = Get-Item -LiteralPath $marker -Force -ErrorAction Stop
    if ($markerItem.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        throw "stage marker cannot be a reparse point: $Name"
    }
    $record = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
    $expectedStage = $Name.Substring(3) -replace '-', ''
    if ($record.schema -ne 1 -or $record.stage -ine $expectedStage) {
        throw "stage marker content is invalid: $Name"
    }
    return $record
}

function Complete-Stage {
    param(
        [Parameter(Mandatory)][IO.DirectoryInfo]$Directory,
        [Parameter(Mandatory)][hashtable]$Details
    )

    $record = [ordered]@{
        schema = 2
        stage = $Stage
        completed_at_utc = [DateTime]::UtcNow.ToString('O')
        details = $Details
    }
    $record | ConvertTo-Json -Depth 6 | Set-Content `
        -LiteralPath (Join-Path $Directory 'completed.json') -Encoding utf8NoBOM
}

function Export-SystemEvidence {
    param([Parameter(Mandatory)][IO.DirectoryInfo]$Directory)

    Get-ComputerInfo | ConvertTo-Json -Depth 4 | Set-Content `
        -LiteralPath (Join-Path $Directory 'computer-info.json') -Encoding utf8NoBOM
    Get-NetAdapter -IncludeHidden | Select-Object Name, InterfaceDescription, InterfaceGuid,
        InterfaceIndex, Status, MacAddress, LinkSpeed, DriverInformation |
        ConvertTo-Json -Depth 4 | Set-Content `
        -LiteralPath (Join-Path $Directory 'net-adapters.json') -Encoding utf8NoBOM
    Get-NetRoute | Select-Object DestinationPrefix, NextHop, InterfaceIndex, RouteMetric,
        Protocol, State, PolicyStore | ConvertTo-Json -Depth 4 | Set-Content `
        -LiteralPath (Join-Path $Directory 'net-routes.json') -Encoding utf8NoBOM
    Get-PnpDevice -Class Net | Select-Object Status, Class, FriendlyName, InstanceId,
        Problem, Present | ConvertTo-Json -Depth 4 | Set-Content `
        -LiteralPath (Join-Path $Directory 'net-devices.json') -Encoding utf8NoBOM
    Get-WindowsDriver -Online | Select-Object Driver, OriginalFileName, ProviderName,
        ClassName, Version, Date | ConvertTo-Json -Depth 4 | Set-Content `
        -LiteralPath (Join-Path $Directory 'windows-drivers.json') -Encoding utf8NoBOM
}

function Get-BootTimeUtc {
    return (Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop).
        LastBootUpTime.ToUniversalTime()
}

function Assert-ExactHealthyXsnet {
    $devices = @(Get-XsnetDevices)
    $packages = @(Get-XsnetDriverPackages)
    $state = Read-XsnetInstallState
    if ($devices.Count -ne 1 -or $devices[0].Status -ne 'OK' -or
        $devices[0].Class -ne 'Net' -or $packages.Count -ne 1 -or
        $packages[0].Driver -cne $state.published_inf -or
        $packages[0].Version.ToString() -cne $state.driver_version -or
        $state.abi_version -ne (Get-XsnetAbiVersion)) {
        throw 'xsnet state is not exactly one healthy Net device and one driver package'
    }
    return [ordered]@{
        device = $devices[0]
        package = $packages[0]
        state = $state
    }
}

function Write-EvidenceManifest {
    $manifestPath = Join-Path $script:RunRoot 'evidence-sha256.json'
    if (Test-Path -LiteralPath $manifestPath) {
        throw 'evidence hash manifest already exists'
    }
    $entries = [ordered]@{}
    foreach ($item in Get-ChildItem -LiteralPath $script:RunRoot -File -Recurse | Sort-Object FullName) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "evidence file cannot be a reparse point: $($item.FullName)"
        }
        $relativePath = [IO.Path]::GetRelativePath($script:RunRoot, $item.FullName)
        $entries[$relativePath] = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash
    }
    [ordered]@{
        schema = 1
        generated_at_utc = [DateTime]::UtcNow.ToString('O')
        files = $entries
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
}

$targetIdentity = Assert-ApprovedTestTarget
$installerRoot = Resolve-RealPath -Path (
    Join-Path $PSScriptRoot '..\..\installers\windows') -Directory
$installer = Resolve-RealPath -Path (Join-Path $installerRoot 'install-xsnet-test.ps1')
$uninstaller = Resolve-RealPath -Path (Join-Path $installerRoot 'uninstall-xsnet-test.ps1')
$module = Resolve-RealPath -Path (Join-Path $installerRoot 'XsnetTestInstaller.psm1')
$runPath = [IO.Path]::GetFullPath($RunDirectory)

if ($Stage -eq 'Initialize') {
    $parentPath = Split-Path -Parent $runPath
    $parent = Resolve-RealPath -Path $parentPath -Directory
    if (-not $runPath.StartsWith($parent + [IO.Path]::DirectorySeparatorChar,
            [StringComparison]::OrdinalIgnoreCase)) {
        throw 'run directory must be a new child of its existing parent'
    }
    if (Test-Path -LiteralPath $runPath) {
        throw 'run directory must not already exist'
    }
    Import-Module $module -Force
    if (@(Get-XsnetDevices).Count -ne 0 -or
        @(Get-XsnetDriverPackages).Count -ne 0) {
        throw 'xsnet must not exist before target validation initialization'
    }
    $script:RunRoot = (New-Item -ItemType Directory -Path $runPath).FullName
    $manifest = [ordered]@{
        schema = 1
        test_only = $true
        recovery_assertion_only = $true
        target_type = $TargetType
        recovery_identifier = $recoveryIdentifier
        computer_name = $env:COMPUTERNAME
        target_identity = $targetIdentity
        physical_recovery = if ($TargetType -eq 'PhysicalMachine') {
            [ordered]@{
                system_image_id = $SystemImageId
                recovery_media_id = $RecoveryMediaId
                disk_recovery_receipt_id = $DiskRecoveryReceiptId
                recovery_operator_id = $RecoveryOperatorId
                external_system_image_asserted = $true
                bootable_recovery_media_asserted = $true
                disk_recovery_material_asserted = $true
                onsite_recovery_operator_asserted = $true
            }
        } else { $null }
        disk_protection = Get-DiskProtectionEvidence
        initialized_at_utc = [DateTime]::UtcNow.ToString('O')
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content `
        -LiteralPath (Join-Path $script:RunRoot 'run.json') -Encoding utf8NoBOM
    $stageDirectory = New-StageDirectory -Name '00-initialize'
    Export-SystemEvidence -Directory $stageDirectory
    Complete-Stage -Directory $stageDirectory -Details @{
        boot_time_utc = (Get-BootTimeUtc).ToString('O')
    }
    return
}

$script:RunRoot = Resolve-RealPath -Path $runPath -Directory
$null = Read-RunManifest
Import-Module $module -Force

switch ($Stage) {
    'Install' {
        $null = Assert-CompletedStage -Name '00-initialize'
        if ([string]::IsNullOrWhiteSpace($PackageDirectory) -or
            [string]::IsNullOrWhiteSpace($ExpectedSignerThumbprint) -or
            [string]::IsNullOrWhiteSpace($ExpectedDriverVersion) -or
            [string]::IsNullOrWhiteSpace($DevGenPath)) {
            throw 'Install requires package, driver version, signer thumbprint, and DevGen path'
        }
        $stageDirectory = New-StageDirectory -Name '10-install'
        & $installer -PackageDirectory $PackageDirectory `
            -ExpectedSignerThumbprint $ExpectedSignerThumbprint -DevGenPath $DevGenPath `
            -ExpectedDriverVersion $ExpectedDriverVersion `
            -AllowTestSignedPackage -Confirm:$false *>&1 | Tee-Object `
            -LiteralPath (Join-Path $stageDirectory 'install.log')
        $installed = Assert-ExactHealthyXsnet
        Export-SystemEvidence -Directory $stageDirectory
        Complete-Stage -Directory $stageDirectory -Details @{
            device_instance_id = [string]$installed.device.InstanceId
            published_inf = [string]$installed.package.Driver
            driver_version = [string]$installed.state.driver_version
            abi_version = [int]$installed.state.abi_version
        }
    }
    'EnableVerifier' {
        $null = Assert-CompletedStage -Name '10-install'
        $null = Assert-ExactHealthyXsnet
        $stageDirectory = New-StageDirectory -Name '20-enable-verifier'
        $verifier = Resolve-RealPath -Path "$env:SystemRoot\System32\verifier.exe"
        Invoke-NativeTool -Path $verifier -Arguments @('/standard', '/driver', 'xsnet.dll') `
            -LogPath (Join-Path $stageDirectory 'verifier-standard.log') `
            -AcceptedExitCodes @(0, 2)
        Invoke-NativeTool -Path $verifier -Arguments @('/bootmode', 'oneboot') `
            -LogPath (Join-Path $stageDirectory 'verifier-bootmode.log') `
            -AcceptedExitCodes @(0, 2)
        Invoke-NativeTool -Path $verifier -Arguments @('/querysettings') `
            -LogPath (Join-Path $stageDirectory 'verifier-querysettings.log')
        Complete-Stage -Directory $stageDirectory -Details @{
            boot_time_utc = (Get-BootTimeUtc).ToString('O')
            reboot_required = $true
        }
    }
    'CollectVerifier' {
        $enable = Assert-CompletedStage -Name '20-enable-verifier'
        if ((Get-BootTimeUtc) -le [DateTime]::Parse($enable.details.boot_time_utc).ToUniversalTime()) {
            throw 'test target must reboot after enabling Driver Verifier'
        }
        $null = Assert-ExactHealthyXsnet
        $stageDirectory = New-StageDirectory -Name '30-collect-verifier'
        $verifier = Resolve-RealPath -Path "$env:SystemRoot\System32\verifier.exe"
        Invoke-NativeTool -Path $verifier -Arguments @('/query') `
            -LogPath (Join-Path $stageDirectory 'verifier-query.log')
        Invoke-NativeTool -Path $verifier -Arguments @('/querysettings') `
            -LogPath (Join-Path $stageDirectory 'verifier-querysettings.log')
        Export-SystemEvidence -Directory $stageDirectory
        Get-WinEvent -FilterHashtable @{
            LogName = 'System'
            StartTime = [DateTime]::Parse($enable.completed_at_utc).ToLocalTime()
            Level = 1, 2, 3
        } -ErrorAction SilentlyContinue | Select-Object TimeCreated, Id, LevelDisplayName,
            ProviderName, Message | ConvertTo-Json -Depth 4 | Set-Content `
            -LiteralPath (Join-Path $stageDirectory 'system-errors.json') -Encoding utf8NoBOM
        Complete-Stage -Directory $stageDirectory -Details @{
            scenario_results_included = $false
            acceptance_claimed = $false
        }
    }
    'DisableVerifier' {
        $null = Assert-CompletedStage -Name '30-collect-verifier'
        $stageDirectory = New-StageDirectory -Name '40-disable-verifier'
        $verifier = Resolve-RealPath -Path "$env:SystemRoot\System32\verifier.exe"
        Invoke-NativeTool -Path $verifier -Arguments @('/reset') `
            -LogPath (Join-Path $stageDirectory 'verifier-reset.log') `
            -AcceptedExitCodes @(0, 2)
        Complete-Stage -Directory $stageDirectory -Details @{
            boot_time_utc = (Get-BootTimeUtc).ToString('O')
            reboot_required = $true
        }
    }
    'Uninstall' {
        $disable = Assert-CompletedStage -Name '40-disable-verifier'
        if ((Get-BootTimeUtc) -le [DateTime]::Parse($disable.details.boot_time_utc).ToUniversalTime()) {
            throw 'test target must reboot after disabling Driver Verifier'
        }
        $stageDirectory = New-StageDirectory -Name '50-uninstall'
        $verifier = Resolve-RealPath -Path "$env:SystemRoot\System32\verifier.exe"
        Invoke-NativeTool -Path $verifier -Arguments @('/querysettings') `
            -LogPath (Join-Path $stageDirectory 'verifier-querysettings.log')
        & $uninstaller -Confirm:$false *>&1 | Tee-Object `
            -LiteralPath (Join-Path $stageDirectory 'uninstall.log')
        if (@(Get-XsnetDevices).Count -ne 0 -or
            @(Get-XsnetDriverPackages).Count -ne 0) {
            throw 'xsnet residual remains after uninstall'
        }
        Export-SystemEvidence -Directory $stageDirectory
        Complete-Stage -Directory $stageDirectory -Details @{
            xsnet_devices = 0
            xsnet_driver_packages = 0
            acceptance_claimed = $false
        }
        Write-EvidenceManifest
    }
}
