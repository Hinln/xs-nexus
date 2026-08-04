# XS Nexus Windows bootstrap template. The release builder substitutes the manifest digest before
# this file is published as https://vpn.qinwen.co/install.
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$ControllerUrl = 'https://vpn.qinwen.co/'
$ReleaseBaseUrl = 'https://vpn.qinwen.co/downloads/windows/stable'
$UpdateSigningPublicKeyUrl = 'https://vpn.qinwen.co/downloads/linux/stable/release-public-key.pem'
$ReleaseVersion = '0.1.0'
$ReleaseTarget = 'x86_64-pc-windows-msvc'
$ManifestName = "xs-nexus-$ReleaseVersion-$ReleaseTarget.manifest.json"
$ArchiveName = "xs-nexus-$ReleaseVersion-$ReleaseTarget.zip"
$ExpectedManifestSha256 = '__RELEASE_MANIFEST_SHA256__'
$ExpectedUpdateSigningPublicKeySha256 = 'b987e95acebaf2ff24d08bab17ae6a3cc60cc89f9805920416a3ac8cabba5253'
$InstallRoot = 'C:\ProgramData\XS Nexus'
$ServiceName = 'XsNexusAgent'
$WintunSignerSubjectPattern = '(^|,\s*)CN=WireGuard LLC(?:,|$)'

function Fail([string] $Message) {
    throw "XS Nexus installation failed: $Message"
}

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) {
        Fail $Message
    }
}

function Get-Sha256([string] $Path) {
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Invoke-HttpsDownload([string] $Uri, [string] $Destination) {
    Assert-True ($Uri.StartsWith('https://', [System.StringComparison]::Ordinal)) 'refusing a non-HTTPS release URL'
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination -MaximumRedirection 0 -ErrorAction Stop
    $item = Get-Item -LiteralPath $Destination -Force -ErrorAction Stop
    Assert-True (-not $item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'download did not create a regular file'
}

function Set-RestrictedAcl([string] $Path) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    Assert-True (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'refusing to change ACL on a reparse point'
    if ($item.PSIsContainer) {
        $grants = @('*S-1-5-18:(OI)(CI)F', '*S-1-5-32-544:(OI)(CI)F')
    } else {
        $grants = @('*S-1-5-18:F', '*S-1-5-32-544:F')
    }
    & icacls.exe $Path /inheritance:r | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Fail "unable to remove inherited access from $Path"
    }
    & icacls.exe $Path /grant:r $grants | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Fail "unable to protect $Path"
    }
}

function Set-AgentPrivateFileAcl([string] $Path) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    Assert-True (-not $item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'agent private storage path must be a regular file'
    $expectedSddl = 'D:P(A;;FA;;;SY)(A;;FA;;;BA)'
    $acl = Get-Acl -LiteralPath $Path -ErrorAction Stop
    $acl.SetSecurityDescriptorSddlForm($expectedSddl, [Security.AccessControl.AccessControlSections]::Access)
    Set-Acl -LiteralPath $Path -AclObject $acl -ErrorAction Stop
    $actualSddl = (Get-Acl -LiteralPath $Path -ErrorAction Stop).GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
    Assert-True ($actualSddl -eq $expectedSddl -or $actualSddl -eq 'D:PAI(A;;FA;;;SY)(A;;FA;;;BA)') 'agent private storage ACL verification failed'
}

function Set-AgentPrivateDirectoryAcl([string] $Path) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    Assert-True ($item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'agent private storage path must be a regular directory'
    $expectedSddl = 'D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)'
    $acl = Get-Acl -LiteralPath $Path -ErrorAction Stop
    $acl.SetSecurityDescriptorSddlForm($expectedSddl, [Security.AccessControl.AccessControlSections]::Access)
    Set-Acl -LiteralPath $Path -AclObject $acl -ErrorAction Stop
    $actualSddl = (Get-Acl -LiteralPath $Path -ErrorAction Stop).GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
    Assert-True ($actualSddl -eq $expectedSddl -or $actualSddl -eq 'D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)') 'agent private storage directory ACL verification failed'
}

function Assert-ExactPayloadTree([string] $Root) {
    $expectedFiles = @('xs-agent.exe', 'xs.exe', 'wintun.dll', 'PAYLOAD.SHA256', 'THIRD_PARTY\WINTUN-LICENSE.txt')
    $expectedDirectories = @('THIRD_PARTY')
    $actualFiles = @()
    $actualDirectories = @()
    foreach ($item in Get-ChildItem -LiteralPath $Root -Recurse -Force) {
        Assert-True (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) 'release payload contains a reparse point'
        $relative = $item.FullName.Substring($Root.Length + 1)
        if ($item.PSIsContainer) {
            Assert-True ($expectedDirectories -contains $relative) 'release payload contains an unexpected directory'
            $actualDirectories += $relative
        } else {
            Assert-True ($expectedFiles -contains $relative) 'release payload contains an unexpected file'
            $actualFiles += $relative
        }
    }
    Assert-True ((@($actualFiles | Sort-Object) -join "`n") -eq (@($expectedFiles | Sort-Object) -join "`n")) 'release payload file set is incomplete'
    Assert-True ((@($actualDirectories | Sort-Object) -join "`n") -eq (@($expectedDirectories | Sort-Object) -join "`n")) 'release payload directory set is invalid'
}

function Get-NodeName {
    $value = [Environment]::MachineName -replace '[^A-Za-z0-9._-]', '-'
    $value = $value.Trim('-')
    if ([string]::IsNullOrWhiteSpace($value)) {
        return 'windows-node'
    }
    return $value.Substring(0, [Math]::Min(63, $value.Length))
}

function Wait-AgentReady([string] $CliPath) {
    for ($attempt = 1; $attempt -le 20; $attempt++) {
        & $CliPath status *> $null
        if ($LASTEXITCODE -eq 0) {
            return
        }
        Start-Sleep -Seconds 1
    }
    Fail 'service did not become ready within 20 seconds'
}

$principal = [Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
Assert-True ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) 'start PowerShell as Administrator'
Assert-True ([Environment]::Is64BitOperatingSystem) 'only 64-bit Windows is supported'
Assert-True ($ExpectedManifestSha256 -match '^[0-9a-f]{64}$') 'bootstrap manifest fingerprint is unavailable'
Assert-True ($ExpectedUpdateSigningPublicKeySha256 -match '^[0-9a-f]{64}$') 'update signing key fingerprint is invalid'

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("xs-nexus-bootstrap-" + [Guid]::NewGuid().ToString('N'))
$tokenPath = Join-Path $temporaryRoot 'enrollment.token'
$createdInstallRoot = $false
$createdService = $false
$bstr = [IntPtr]::Zero

try {
    New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
    Set-AgentPrivateDirectoryAcl $temporaryRoot
    $manifestPath = Join-Path $temporaryRoot $ManifestName
    $archivePath = Join-Path $temporaryRoot $ArchiveName
    $updateSigningPublicKeyPath = Join-Path $temporaryRoot 'release-public-key.pem'
    Invoke-HttpsDownload "$ReleaseBaseUrl/$ManifestName" $manifestPath
    Assert-True ((Get-Sha256 $manifestPath) -eq $ExpectedManifestSha256) 'release manifest hash verification failed'
    Invoke-HttpsDownload $UpdateSigningPublicKeyUrl $updateSigningPublicKeyPath
    Assert-True ((Get-Item -LiteralPath $updateSigningPublicKeyPath).Length -le 8192) 'update signing public key is oversized'
    Assert-True ((Get-Sha256 $updateSigningPublicKeyPath) -eq $ExpectedUpdateSigningPublicKeySha256) 'update signing public key verification failed'

    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert-True ($manifest.schema_version -eq 1) 'release manifest schema is unsupported'
    Assert-True ($manifest.product -eq 'xs-nexus') 'release manifest product is invalid'
    Assert-True ($manifest.version -eq $ReleaseVersion) 'release manifest version is invalid'
    Assert-True ($manifest.platform -eq 'windows') 'release manifest platform is invalid'
    Assert-True ($manifest.architecture -eq 'x86_64') 'release manifest architecture is invalid'
    Assert-True ($manifest.target -eq $ReleaseTarget) 'release manifest target is invalid'
    Assert-True ($manifest.archive -eq $ArchiveName) 'release manifest archive is invalid'
    Assert-True ($manifest.archive_sha256 -match '^[0-9a-f]{64}$') 'release archive hash is invalid'
    Assert-True ($manifest.archive_size -is [int] -or $manifest.archive_size -is [long]) 'release archive size type is invalid'
    $archiveSize = [long] $manifest.archive_size
    Assert-True ($archiveSize -gt 0 -and $archiveSize -le 536870912) 'release archive size is invalid'
    Assert-True ($manifest.wintun.version -eq '0.14.1') 'Wintun version is invalid'
    Assert-True ($manifest.wintun.sha256 -match '^[0-9a-f]{64}$') 'Wintun hash is invalid'

    Invoke-HttpsDownload "$ReleaseBaseUrl/$ArchiveName" $archivePath
    Assert-True ((Get-Item -LiteralPath $archivePath).Length -eq $archiveSize) 'release archive size verification failed'
    Assert-True ((Get-Sha256 $archivePath) -eq $manifest.archive_sha256) 'release archive hash verification failed'

    $releaseRoot = Join-Path $temporaryRoot 'release'
    Expand-Archive -LiteralPath $archivePath -DestinationPath $releaseRoot -ErrorAction Stop
    Assert-ExactPayloadTree $releaseRoot
    $agentPath = Join-Path $releaseRoot 'xs-agent.exe'
    $cliPath = Join-Path $releaseRoot 'xs.exe'
    $wintunPath = Join-Path $releaseRoot 'wintun.dll'
    $licensePath = Join-Path $releaseRoot 'THIRD_PARTY\WINTUN-LICENSE.txt'
    foreach ($path in @($agentPath, $cliPath, $wintunPath, $licensePath)) {
        Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "release payload is missing $path"
    }
    $payload = Get-Content -LiteralPath (Join-Path $releaseRoot 'PAYLOAD.SHA256') -Encoding ASCII
    $expectedPayload = @('xs-agent.exe', 'xs.exe', 'wintun.dll', 'THIRD_PARTY\WINTUN-LICENSE.txt')
    Assert-True ($payload.Count -eq $expectedPayload.Count) 'release payload manifest is invalid'
    $actualPayload = @()
    foreach ($line in $payload) {
        $parts = $line -split '  ', 2
        Assert-True ($parts.Count -eq 2 -and $parts[0] -match '^[0-9a-f]{64}$') 'release payload manifest is invalid'
        Assert-True ($expectedPayload -contains $parts[1]) 'release payload manifest is invalid'
        Assert-True (-not ($actualPayload -contains $parts[1])) 'release payload manifest is invalid'
        $actualPayload += $parts[1]
        $payloadPath = Join-Path $releaseRoot $parts[1]
        Assert-True (Test-Path -LiteralPath $payloadPath -PathType Leaf) 'release payload file is invalid'
        Assert-True ((Get-Sha256 $payloadPath) -eq $parts[0]) 'release payload hash verification failed'
    }
    Assert-True (
        (@($actualPayload | Sort-Object) -join "`n") -eq
        (@($expectedPayload | Sort-Object) -join "`n")
    ) 'release payload manifest is incomplete'
    Assert-True ((Get-Sha256 $wintunPath) -eq $manifest.wintun.sha256) 'Wintun library hash verification failed'
    $signature = Get-AuthenticodeSignature -LiteralPath $wintunPath
    Assert-True ($signature.Status -eq 'Valid') 'Wintun code signature is invalid'
    Assert-True ($signature.SignerCertificate.Subject -match $WintunSignerSubjectPattern) 'Wintun signer identity is invalid'

    Assert-True (-not (Test-Path -LiteralPath $InstallRoot)) 'an existing XS Nexus installation must be uninstalled before reinstalling'
    New-Item -ItemType Directory -Path $InstallRoot | Out-Null
    $createdInstallRoot = $true
    Set-RestrictedAcl $InstallRoot
    $binDirectory = Join-Path $InstallRoot 'bin'
    $stateDirectory = Join-Path $InstallRoot 'state'
    New-Item -ItemType Directory -Path $binDirectory, $stateDirectory | Out-Null
    Copy-Item -LiteralPath $agentPath, $cliPath, $wintunPath -Destination $binDirectory -Force
    Copy-Item -LiteralPath $licensePath -Destination (Join-Path $InstallRoot 'WINTUN-LICENSE.txt') -Force
    Set-RestrictedAcl $binDirectory
    Set-RestrictedAcl $stateDirectory
    Set-RestrictedAcl (Join-Path $InstallRoot 'WINTUN-LICENSE.txt')

    $installedAgent = Join-Path $binDirectory 'xs-agent.exe'
    $installedCli = Join-Path $binDirectory 'xs.exe'
    $installedWintun = Join-Path $binDirectory 'wintun.dll'
    $configPath = Join-Path $InstallRoot 'agent.json'
    $pinnedUpdateSigningPublicKeyPath = Join-Path $InstallRoot 'release-public-key.pem'
    Copy-Item -LiteralPath $updateSigningPublicKeyPath -Destination $pinnedUpdateSigningPublicKeyPath -Force
    Set-AgentPrivateFileAcl $pinnedUpdateSigningPublicKeyPath
    $config = [ordered]@{
        controller_url = $ControllerUrl
        node_name = Get-NodeName
        device_type = 'windows'
        state_directory = $stateDirectory
        runtime_directory = $InstallRoot
        interface_name = 'xsn0'
        mtu = 1280
        control_sync_interval_seconds = 15
        update_channel = 'stable'
        update_signing_public_key_path = $pinnedUpdateSigningPublicKeyPath
        windows_wintun = [ordered]@{
            library_path = $installedWintun
            sha256 = $manifest.wintun.sha256
        }
    }
    [IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false))
    Set-RestrictedAcl $configPath

    $credentialInput = Read-Host -Prompt 'Enrollment Token' -AsSecureString
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($credentialInput)
    $value = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    Assert-True ($value -match '^xsenr1_\S{10,89}$') 'Enrollment Token format is invalid'
    [IO.File]::WriteAllText($tokenPath, $value, [Text.UTF8Encoding]::new($false))
    Set-AgentPrivateFileAcl $tokenPath
    Remove-Variable value -ErrorAction SilentlyContinue
    & $installedAgent enroll --config $configPath --token-file $tokenPath
    Assert-True ($LASTEXITCODE -eq 0) 'agent enrollment was rejected'
    Remove-Item -LiteralPath $tokenPath -Force -ErrorAction SilentlyContinue

    Assert-True (-not (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue)) 'an existing XS Nexus service must be removed before installation'
    New-Service -Name $ServiceName -BinaryPathName ('"{0}" service --config "{1}"' -f $installedAgent, $configPath) -DisplayName 'XS Nexus Agent' -Description 'XS Nexus virtual network agent' -StartupType Automatic
    $createdService = $true
    & sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/5000/""/0 | Out-Null
    Assert-True ($LASTEXITCODE -eq 0) 'unable to configure service recovery'
    Start-Service -Name $ServiceName
    Wait-AgentReady $installedCli
    Write-Host 'XS Nexus installation completed.'
    & $installedCli status
} catch {
    if ($createdService -and (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue)) {
        Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
        & sc.exe delete $ServiceName | Out-Null
    }
    if ($createdInstallRoot -and (Test-Path -LiteralPath $InstallRoot)) {
        Remove-Item -LiteralPath $InstallRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    throw
} finally {
    if ($bstr -ne [IntPtr]::Zero) {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
