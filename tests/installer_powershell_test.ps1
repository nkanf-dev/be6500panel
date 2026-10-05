#requires -Version 5.1
# Offline PowerShell assertions. No router, network, Pester or WSL needed.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
. (Join-Path $Root 'scripts/enable-ssh.ps1')
. (Join-Path $Root 'scripts/install-panel.ps1')
function Assert-Equal($Actual, $Expected, [string]$Label) {
    if ($Actual -cne $Expected) { throw ('FAIL ' + $Label + ': expected [' + $Expected + '] actual [' + $Actual + ']') }
}
function Assert-True([bool]$Value, [string]$Label) { if (-not $Value) { throw ('FAIL ' + $Label) } }
function Assert-Throws([scriptblock]$Action, [string]$Label) {
    $thrown = $false
    try { & $Action } catch { $thrown = $true }
    Assert-True $thrown $Label
}
Assert-Equal (Get-InitialRootPassword ' 57941/F4U711396 ') '8ce67217' 'RN02 initial password'
Assert-Equal (Get-InitialRootPassword '123456') 'ec877dd3' 'fixed salt without slash'
Assert-Throws { Get-InitialRootPassword '' } 'empty SN'
Assert-Throws { Assert-RouterAddress '127.0.0.1; whoami' } 'address injection'
Assert-Throws { Assert-RouterAddress '192.168.031.1' } 'noncanonical IPv4'
Assert-RouterAddress '192.168.31.1'
Assert-ReleaseNames 'nkanf-dev/be6500panel' 'v0.3.0' 'be6500panel-armv7.tar.gz'
Assert-Throws { Assert-ReleaseNames 'nkanf-dev/be6500panel' 'v0.3.0' '../panel.tar.gz' } 'asset traversal'
Assert-Throws { Assert-ReleaseNames 'nkanf-dev/be6500panel' 'v0.3.0/xx' 'be6500panel-armv7.tar.gz' } 'tag injection'
$options = @(Get-SshOptions 2222 'C:\keys\rescue' -KeyOnly)
Assert-True ($options -contains 'BatchMode=yes') 'key authentication is noninteractive'
Assert-True ($options -contains 'StrictHostKeyChecking=ask') 'host key trust retained'
Assert-True (($options -join ' ') -notmatch 'UserKnownHostsFile|StrictHostKeyChecking=no') 'do not discard known hosts'
Assert-Equal (Quote-NativeArgument 'C:\keys with space\rescue') '"C:\keys with space\rescue"' 'native quoting'
$script:Requests = @(); $script:Firmware = '1.0.43'; $script:Hardware = 'RN02'; $script:FailAt = 0
function Invoke-RouterJson([string]$Uri, [string]$Body = '') {
    $script:Requests += [pscustomobject]@{ Uri=$Uri; Body=$Body }
    if ($Body.Length -eq 0) { return [pscustomobject]@{code=0;hardware=$script:Hardware;romversion=$script:Firmware} }
    if ($script:FailAt -eq $script:Requests.Count - 1) { throw 'mock router rejection' }
    return [pscustomobject]@{code=0}
}
$stok = '0123456789abcdef0123456789abcdef'
Enable-RouterSsh '192.168.31.1' $stok
Assert-Equal $script:Requests.Count 5 'one probe + four steps'
$expectedBodies = @(
 "uid=1234&key=1234'%0Anvram%20set%20ssh_en%3D1'",
 "uid=1234&key=1234'%0Anvram%20commit'",
 "uid=1234&key=1234'%0Ased%20-i%20's%2Fchannel%3D.*%2Fchannel%3D%22debug%22%2Fg'%20%2Fetc%2Finit.d%2Fdropbear'",
 "uid=1234&key=1234'%0A%2Fetc%2Finit.d%2Fdropbear%20start'"
)
for ($i=0; $i -lt 4; $i++) { Assert-Equal $script:Requests[$i+1].Body $expectedBodies[$i] ('fixed body ' + $i) }
$script:Requests=@(); $script:Firmware='1.0.64'
Assert-Throws { Enable-RouterSsh '192.168.31.1' $stok } 'firmware guard'
Assert-Equal $script:Requests.Count 1 'firmware rejected without writes'
$script:Requests=@(); $script:Firmware='1.0.43'; $script:Hardware='OTHER'
Assert-Throws { Enable-RouterSsh '192.168.31.1' $stok } 'model guard'
Assert-Equal $script:Requests.Count 1 'model rejected without writes'
$script:Requests=@(); $script:Hardware='RN02'; $script:FailAt=2
Assert-Throws { Enable-RouterSsh '192.168.31.1' $stok } 'stop on rejected step'
Assert-Equal $script:Requests.Count 3 'no later step after failure'
$script:Requests=@()
Assert-Throws { Enable-RouterSsh '192.168.31.1' 'bad' } 'STOK format'
Assert-Equal $script:Requests.Count 0 'no malformed token request'

$testWork = Join-Path ([IO.Path]::GetTempPath()) ('be6500panel-pstest-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testWork
try {
    $archive = Join-Path $testWork 'panel.tar.gz'; $sums=Join-Path $testWork 'SHA256SUMS'
    [IO.File]::WriteAllText($archive, 'offline archive fixture')
    $digest=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($sums, $digest + '  be6500panel-armv7.tar.gz' + "`n")
    Assert-Equal (Get-VerifiedReleaseHash $archive $sums 'be6500panel-armv7.tar.gz') $digest 'real local SHA256'
    [IO.File]::AppendAllText($sums, $digest + '  be6500panel-armv7.tar.gz' + "`n")
    Assert-Throws { Get-VerifiedReleaseHash $archive $sums 'be6500panel-armv7.tar.gz' } 'duplicate checksum'
    [IO.File]::WriteAllText($sums, ('0'*64) + '  be6500panel-armv7.tar.gz' + "`n")
    Assert-Throws { Get-VerifiedReleaseHash $archive $sums 'be6500panel-armv7.tar.gz' } 'mismatch checksum'
    $pub=Join-Path $testWork 'key.pub'
    [IO.File]::WriteAllText($pub, 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQCy offline')
    $null=Assert-OwnedPublicKey $pub
    [IO.File]::WriteAllText($pub, 'command="id" ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQCy')
    Assert-Throws { Assert-OwnedPublicKey $pub } 'public key options rejected'
    [IO.File]::WriteAllText($pub, '-----BEGIN OPENSSH PRIVATE KEY-----')
    Assert-Throws { Assert-OwnedPublicKey $pub } 'private key rejected'

    # Native SSH wrapper test is a function mock, never invokes a device.
    $script:NativeArgs=@()
    function ssh { $script:NativeArgs=@($args); $global:LASTEXITCODE=0; 'mock-ok' }
    Assert-Equal (Invoke-InstallerSsh '192.168.31.1' 2222 'printf verified' 'C:\rescue' -KeyOnly) 'mock-ok' 'SSH wrapper'
    Assert-Equal $script:NativeArgs[-2] 'root@192.168.31.1' 'separate SSH destination arg'
    Assert-Equal $script:NativeArgs[-1] 'printf verified' 'single remote command arg'

    # Full host orchestration with all external operations mocked.
    $script:Events=@(); $script:Mode='fresh'; $script:FailApply=$false; $script:FailKey=$false; $script:BackupWork=''
    function Get-OwnedRescueKey([string]$RequestedKey) { return 'mock-rescue-key' }
    function Get-BoundedDownload([string]$Uri,[string]$Destination,[long]$MaxBytes) {
        $script:Events += ('download ' + $MaxBytes)
        [IO.File]::WriteAllText($Destination, 'fixture')
    }
    function Get-VerifiedReleaseHash([string]$Archive,[string]$Sums,[string]$Name) { $script:Events+='checksum'; return ('a'*64) }
    function Copy-ToRouter([string]$Address,[int]$SshPort,[string]$LocalPath,[string]$RemotePath,[string]$Key='') { $script:Events+=('upload ' + $RemotePath) }
    function Copy-FromRouter([string]$Address,[int]$SshPort,[string]$RemotePath,[string]$LocalPath,[string]$Key='') { $script:BackupWork=Split-Path -Parent $LocalPath; $script:Events+=('backup ' + $RemotePath); [IO.File]::WriteAllText($LocalPath, 'old') }
    function Get-PreviousManagerHash([string]$Work) { return ('b'*64) }
    function Read-Host { ConvertTo-SecureString 'offline-strong-password' -AsPlainText -Force }
    function Send-PanelPassword([string]$Address,[string]$Key,[string]$Stage,[Security.SecureString]$Password,[int]$SshPort=2222,[switch]$Interactive) { $script:Events+='private-password' }
    function Invoke-InstallerSsh([string]$Address,[int]$SshPort,[string]$Command,[string]$Key='',[switch]$KeyOnly) {
        $script:Events += ('ssh ' + $SshPort + ' ' + [bool]$KeyOnly + ' ' + $Command)
        if ($script:FailKey -and $KeyOnly) { throw 'mock rescue key failure' }
        if ($Command -match '/mode$' -or $Command.StartsWith('if test -e /data/be6500panel')) { return $script:Mode }
        if ($Command -match '/lan-ip$') { return '192.168.31.1' }
        if ($script:FailApply -and $Command -match 'router-install\.sh apply ') { throw 'mock apply failure' }
        return 'ok'
    }
    Invoke-PanelReleaseInstall '192.168.31.1' 22 '' 'nkanf-dev/be6500panel' 'v0.3.0' 'be6500panel-armv7.tar.gz'
    $eventText=$script:Events -join "`n"
    Assert-True ($eventText.IndexOf('checksum') -lt $eventText.IndexOf('upload ')) 'verify before upload'
    Assert-True ($eventText.IndexOf('rescue-verified') -lt $eventText.IndexOf('router-install.sh apply')) 'rescue key verified before apply'
    Assert-True ($eventText.IndexOf('private-password') -lt $eventText.IndexOf('router-install.sh apply')) 'private password before apply'
    Assert-True ($eventText -match '2222 True .*rescue-verified') 'marker written via external key-only port'
    Assert-True ($eventText -match 'router-install.sh verify ') 'final helper verification'
    Assert-True ($eventText -notmatch 'offline-strong-password') 'no password in commands'
    $script:Events=@(); $script:FailKey=$true
    Assert-Throws { Invoke-PanelReleaseInstall '192.168.31.1' 22 '' 'nkanf-dev/be6500panel' 'v0.3.0' 'be6500panel-armv7.tar.gz' } 'rescue login failure'
    Assert-True (($script:Events -join "`n") -notmatch 'router-install.sh apply ') 'failed rescue must not apply'
    $script:Events=@(); $script:FailKey=$false; $script:Mode='upgrade'; $script:FailApply=$true
    Assert-Throws { Invoke-PanelReleaseInstall '192.168.31.1' 22 '' 'nkanf-dev/be6500panel' 'v0.3.0' 'be6500panel-armv7.tar.gz' } 'upgrade failure'
    $eventText=$script:Events -join "`n"
    Assert-True ($eventText -match '/rollback/be6500-panel.sha256') 'old executable SHA supplied'
    Assert-True ($eventText -match 'router-install.sh rollback ') 'upgrade rollback attempted'
    Assert-True ($eventText -notmatch 'private-password') 'upgrade keeps existing password'
    Assert-True ($eventText -notmatch '/(core|history)(/|$)') 'do not copy core/history'
} finally {
    Remove-Item -LiteralPath $testWork -Recurse -Force
    if ($script:BackupWork -ne '' -and (Test-Path -LiteralPath $script:BackupWork)) { Remove-Item -LiteralPath $script:BackupWork -Recurse -Force }
}
Write-Host 'PASS: offline PowerShell installer assertions'
