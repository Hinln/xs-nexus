Set-StrictMode -Version Latest

$script:XsnetHardwareId = 'Root\XSNET'
$script:XsnetInstanceId = 'ROOT\DEVGEN\XSNET'
$script:XsnetStatePath = Join-Path $env:ProgramData 'XS Nexus\xsnet-test-install.json'
$script:XsnetAbiVersion = 1

function Assert-XsnetTestHost {
    if (-not [Environment]::Is64BitOperatingSystem) {
        throw 'xsnet requires a 64-bit Windows host'
    }
    if ([Environment]::OSVersion.Version.Build -lt 26100) {
        throw 'xsnet test installation requires Windows 11 build 26100 or newer'
    }
}

function Invoke-XsnetNative {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][string[]]$Arguments
    )

    $output = & $FilePath @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE`: $($output -join [Environment]::NewLine)"
    }
    return @($output)
}

function Assert-XsnetSignature {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$ExpectedThumbprint
    )

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate) {
        throw "invalid Authenticode signature: $Path"
    }
    $actual = ($signature.SignerCertificate.Thumbprint -replace '[^0-9A-Fa-f]', '').ToUpperInvariant()
    if ($actual -ne $ExpectedThumbprint) {
        throw "unexpected signer thumbprint: $Path"
    }
    $now = Get-Date
    if ($signature.SignerCertificate.NotBefore -gt $now -or
        $signature.SignerCertificate.NotAfter -lt $now) {
        throw "signer certificate is outside its validity period: $Path"
    }
}

function Assert-XsnetDevGen {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if ((Get-Item -LiteralPath $resolved).Attributes -band
        [IO.FileAttributes]::ReparsePoint) {
        throw 'DevGen cannot be a reparse point'
    }
    $signature = Get-AuthenticodeSignature -LiteralPath $resolved
    if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or
        $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
        throw 'DevGen must be the valid Microsoft-signed WDK tool'
    }
    return $resolved
}

function Get-XsnetAbiVersion {
    return $script:XsnetAbiVersion
}

function Get-XsnetInfDriverVersion {
    param([Parameter(Mandatory)][string]$Path)

    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'xsnet INF must be a real file'
    }
    $matches = @(Get-Content -LiteralPath $item.FullName | Where-Object {
        $_ -match '^\s*DriverVer\s*=\s*[^,]+,\s*(?<Version>[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)\s*$'
    })
    if ($matches.Count -ne 1) {
        throw 'xsnet INF must contain exactly one four-part DriverVer'
    }
    return [regex]::Match(
        $matches[0],
        '^\s*DriverVer\s*=\s*[^,]+,\s*(?<Version>[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)\s*$'
    ).Groups['Version'].Value
}

function Get-XsnetDevices {
    $devices = @()
    # DevGen deterministically creates this root-enumerated instance. Querying
    # every present PnP device and then fetching every hardware-ID property can
    # take several minutes on storage-constrained validation VMs.
    foreach ($device in @(Get-PnpDevice -InstanceId $script:XsnetInstanceId `
            -ErrorAction SilentlyContinue)) {
        try {
            $property = Get-PnpDeviceProperty -InstanceId $device.InstanceId `
                -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop
            if (@($property.Data) -contains $script:XsnetHardwareId) {
                $devices += $device
            }
        } catch {
            continue
        }
    }
    return @($devices)
}

function Get-XsnetDriverPackages {
    return @(Get-WindowsDriver -Online | Where-Object {
        [IO.Path]::GetFileName($_.OriginalFileName) -ieq 'xsnet.inf'
    })
}

function Write-XsnetInstallState {
    param([Parameter(Mandatory)][object]$State)

    $directory = Split-Path -Parent $script:XsnetStatePath
    if (Test-Path -LiteralPath $directory) {
        if ((Get-Item -LiteralPath $directory).Attributes -band
            [IO.FileAttributes]::ReparsePoint) {
            throw 'xsnet state directory cannot be a reparse point'
        }
    }
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\icacls.exe" -Arguments @(
        $directory,
        '/inheritance:r',
        '/grant:r',
        '*S-1-5-18:(OI)(CI)F',
        '*S-1-5-32-544:(OI)(CI)F'
    ) | Out-Null
    $temporary = "$script:XsnetStatePath.tmp"
    $State | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $temporary `
        -Encoding utf8NoBOM
    Move-Item -LiteralPath $temporary -Destination $script:XsnetStatePath -Force
}

function Read-XsnetInstallState {
    if (-not (Test-Path -LiteralPath $script:XsnetStatePath -PathType Leaf)) {
        throw 'xsnet test installation state is missing'
    }
    if ((Get-Item -LiteralPath $script:XsnetStatePath).Attributes -band
        [IO.FileAttributes]::ReparsePoint) {
        throw 'xsnet state file cannot be a reparse point'
    }
    $state = Get-Content -LiteralPath $script:XsnetStatePath -Raw | ConvertFrom-Json
    if ($state.schema -ne 2 -or $state.hardware_id -ne $script:XsnetHardwareId -or
        $state.abi_version -ne $script:XsnetAbiVersion -or
        $state.driver_version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$' -or
        $state.published_inf -notmatch '^oem[0-9]+\.inf$') {
        throw 'xsnet test installation state is invalid'
    }
    foreach ($instanceId in @($state.device_instance_ids)) {
        if ($instanceId -notlike 'ROOT\*') {
            throw 'xsnet state contains an invalid device instance ID'
        }
    }
    return $state
}

function Test-XsnetDeviceRemovalTarget {
    param([Parameter(Mandatory)][string]$InstanceId)

    $device = Get-PnpDevice -InstanceId $InstanceId -ErrorAction SilentlyContinue
    if ($null -eq $device) {
        return $false
    }
    $property = Get-PnpDeviceProperty -InstanceId $InstanceId `
        -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction Stop
    if (@($property.Data) -notcontains $script:XsnetHardwareId) {
        throw 'refusing to remove a device that is not Root\XSNET'
    }
    return $true
}

function Test-XsnetDriverRemovalTarget {
    param([Parameter(Mandatory)][string]$PublishedInf)

    $matches = @(Get-WindowsDriver -Online | Where-Object {
        $_.Driver -ieq $PublishedInf
    })
    if ($matches.Count -eq 0) {
        return $false
    }
    if ($matches.Count -ne 1 -or
        [IO.Path]::GetFileName($matches[0].OriginalFileName) -ine 'xsnet.inf') {
        throw 'refusing to remove a driver package that is not xsnet.inf'
    }
    return $true
}

function Remove-XsnetState {
    if (Test-Path -LiteralPath $script:XsnetStatePath -PathType Leaf) {
        Remove-Item -LiteralPath $script:XsnetStatePath -Force
    }
}

Export-ModuleMember -Function *-Xsnet*
