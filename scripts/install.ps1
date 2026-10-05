#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Router = '192.168.31.1',
    [ValidateRange(1,65535)][int]$Port = 22,
    [string]$IdentityFile = '',
    [string]$Repository = 'nkanf-dev/be6500panel',
    [string]$Version = 'v0.3.0',
    [string]$Asset = 'be6500panel-armv7.tar.gz'
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Start-PanelInstaller {
    Write-Host 'be6500panel installer'
    Write-Host '1. Enable SSH (RN02 firmware 1.0.42 / 1.0.43) and install'
    Write-Host '2. SSH is available: install the panel'
    Write-Host '0. Exit'
    $choice = Read-Host 'Choose [1/2/0]'
    switch ($choice) {
        '1' {
            & (Join-Path $PSScriptRoot 'enable-ssh.ps1') -Router $Router
        }
        '2' { }
        '0' { return }
        default { throw 'Invalid installer option.' }
    }
    & (Join-Path $PSScriptRoot 'install-panel.ps1') -Router $Router -Port $Port -IdentityFile $IdentityFile -Repository $Repository -Version $Version -Asset $Asset
}
if ($MyInvocation.InvocationName -ne '.') { Start-PanelInstaller }
