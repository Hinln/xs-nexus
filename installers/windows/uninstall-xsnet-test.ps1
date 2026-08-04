#Requires -Version 7.4
#Requires -RunAsAdministrator

[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'XsnetTestInstaller.psm1') -Force

Assert-XsnetTestHost
$state = Read-XsnetInstallState
if (-not $PSCmdlet.ShouldProcess('ROOT\XSNET', 'Remove xsnet test device and driver package')) {
    return
}
foreach ($instanceId in @($state.device_instance_ids)) {
    if (Test-XsnetDeviceRemovalTarget -InstanceId $instanceId) {
        Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\pnputil.exe" -Arguments @(
            '/remove-device',
            $instanceId,
            '/subtree'
        ) | Out-Null
    }
}
if (Test-XsnetDriverRemovalTarget -PublishedInf $state.published_inf) {
    Invoke-XsnetNative -FilePath "$env:SystemRoot\System32\pnputil.exe" -Arguments @(
        '/delete-driver',
        $state.published_inf,
        '/uninstall',
        '/force'
    ) | Out-Null
}

for ($attempt = 0; $attempt -lt 40; $attempt += 1) {
    if (@(Get-XsnetDevices).Count -eq 0 -and
        @(Get-XsnetDriverPackages).Count -eq 0) {
        Remove-XsnetState
        return
    }
    Start-Sleep -Milliseconds 500
}
throw 'xsnet device or driver package remains after uninstall'
