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
. (Join-Path $PSScriptRoot 'enable-ssh.ps1') -Router $Router

function Assert-ReleaseNames([string]$Repo, [string]$Tag, [string]$Name) {
    if ($Repo -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*$' -or
        $Repo.Contains('..') -or $Tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?$' -or
        $Name -notmatch '^be6500panel-armv7(?:-[A-Za-z0-9._-]+)?\.tar\.gz$' -or $Name.Contains('..')) {
        throw 'Invalid release repository, version or asset name.'
    }
}
function Get-BoundedDownload([string]$Uri, [string]$Destination, [long]$MaxBytes) {
    $response = $null; $input = $null; $output = $null
    $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
        $current = [Uri]$Uri
        for ($redirect = 0; $redirect -le 5; $redirect++) {
            if ($current.Scheme -cne 'https' -or $current.UserInfo -ne '' -or
                ($current.Host -cne 'github.com' -and $current.Host -notmatch '(^|\.)githubusercontent\.com$')) {
                throw 'Release URL rejected.'
            }
            $request = [Net.HttpWebRequest]::Create($current)
            $request.AllowAutoRedirect = $false
            $request.Timeout = 30000; $request.ReadWriteTimeout = 15000
            $request.UserAgent = 'be6500panel-installer'
            $response = $request.GetResponse()
            $status = [int]$response.StatusCode
            if ($status -in @(301,302,303,307,308)) {
                if ($redirect -eq 5 -or [string]::IsNullOrEmpty($response.Headers['Location'])) { throw 'Too many redirects.' }
                $current = New-Object Uri($current, $response.Headers['Location'])
                $response.Dispose(); $response = $null
                continue
            }
            if ($status -ne 200 -or $response.ContentLength -gt $MaxBytes) { throw 'Release download rejected.' }
            $input = $response.GetResponseStream()
            $output = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
            $chunk = New-Object byte[] 65536
            $total = 0L
            while (($count = $input.Read($chunk, 0, $chunk.Length)) -gt 0) {
                $total += $count
                if ($total -gt $MaxBytes -or $clock.Elapsed.TotalSeconds -gt 120) { throw 'Release download exceeds its limit.' }
                $output.Write($chunk, 0, $count)
            }
            if ($total -eq 0) { throw 'Empty release asset.' }
            return
        }
        throw 'No release download.'
    } catch { throw 'Release download failed. Check the release version and network connection.' }
    finally {
        if ($null -ne $output) { $output.Dispose() }
        if ($null -ne $input) { $input.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
    }
}
function Get-VerifiedReleaseHash([string]$Archive, [string]$Sums, [string]$Name) {
    $matchesFound = @()
    foreach ($line in [IO.File]::ReadAllLines($Sums)) {
        if ($line -match '^([a-fA-F0-9]{64}) [ *]([^\r\n]+)$' -and $Matches[2] -ceq $Name) {
            $matchesFound += $Matches[1].ToLowerInvariant()
        }
    }
    if ($matchesFound.Count -ne 1) { throw 'SHA256SUMS must contain exactly one checksum for this asset.' }
    $actual = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $matchesFound[0]) { throw 'Release SHA256 mismatch. Nothing was uploaded.' }
    return $actual
}
function Get-SshOptions([int]$SshPort, [string]$Key = '', [switch]$KeyOnly) {
    $options = @('-p', [string]$SshPort, '-o', 'StrictHostKeyChecking=ask', '-o', 'ConnectTimeout=15',
        '-o', 'HostKeyAlgorithms=+ssh-rsa', '-o', 'PubkeyAcceptedAlgorithms=+ssh-rsa')
    if ($Key.Length -gt 0) { $options += @('-i', $Key) }
    if ($KeyOnly) { $options += @('-o', 'BatchMode=yes', '-o', 'IdentitiesOnly=yes', '-o', 'PasswordAuthentication=no', '-o', 'KbdInteractiveAuthentication=no') }
    return $options
}
function Invoke-InstallerSsh([string]$Address, [int]$SshPort, [string]$Command, [string]$Key = '', [switch]$KeyOnly) {
    $arguments = @(Get-SshOptions $SshPort $Key -KeyOnly:$KeyOnly) + @(('root@' + $Address), $Command)
    $result = @(& ssh @arguments)
    if ($LASTEXITCODE -ne 0) { throw 'SSH operation failed.' }
    return ($result -join "`n")
}
function Copy-ToRouter([string]$Address, [int]$SshPort, [string]$LocalPath, [string]$RemotePath, [string]$Key = '') {
    $arguments = @('-O', '-P', [string]$SshPort, '-o', 'StrictHostKeyChecking=ask', '-o', 'ConnectTimeout=15',
        '-o', 'HostKeyAlgorithms=+ssh-rsa', '-o', 'PubkeyAcceptedAlgorithms=+ssh-rsa')
    if ($Key.Length -gt 0) { $arguments += @('-i', $Key) }
    $arguments += @($LocalPath, ('root@' + $Address + ':' + $RemotePath))
    & scp @arguments
    if ($LASTEXITCODE -ne 0) { throw 'SCP upload failed.' }
}
function Copy-FromRouter([string]$Address, [int]$SshPort, [string]$RemotePath, [string]$LocalPath, [string]$Key = '') {
    $arguments = @('-O', '-P', [string]$SshPort, '-o', 'StrictHostKeyChecking=ask', '-o', 'ConnectTimeout=15',
        '-o', 'HostKeyAlgorithms=+ssh-rsa', '-o', 'PubkeyAcceptedAlgorithms=+ssh-rsa')
    if ($Key.Length -gt 0) { $arguments += @('-i', $Key) }
    $arguments += @(('root@' + $Address + ':' + $RemotePath), $LocalPath)
    & scp @arguments
    if ($LASTEXITCODE -ne 0) { throw 'SCP backup failed. Installation stopped before apply.' }
}
function Assert-OwnedPublicKey([string]$Path) {
    $text = [IO.File]::ReadAllText($Path).Trim()
    if ($text -match '[\r\n]' -or $text -notmatch '^(ssh-rsa|ssh-ed25519|ecdsa-sha2-nistp(256|384|521)) [A-Za-z0-9+/]+={0,2}( [^\r\n]*)?$') {
        throw 'The rescue public key must contain one plain SSH public key, with no options.'
    }
    return $text
}
function Get-OwnedRescueKey([string]$RequestedKey) {
    $key = $RequestedKey
    if ([string]::IsNullOrWhiteSpace($key)) {
        $key = Read-Host 'Rescue private key path (blank creates ~/.ssh/be6500panel_rescue_rsa)'
        if ([string]::IsNullOrWhiteSpace($key)) { $key = Join-Path (Join-Path $HOME '.ssh') 'be6500panel_rescue_rsa' }
    }
    $key = [IO.Path]::GetFullPath($key)
    if (-not (Test-Path -LiteralPath $key -PathType Leaf)) {
        if ((Read-Host 'Generate a new user-owned rescue SSH key at this path [yes/NO]') -cne 'yes') { throw 'A rescue SSH key is required.' }
        $directory = Split-Path -Parent $key
        $null = New-Item -ItemType Directory -Force -Path $directory
        # ssh-keygen prompts for the passphrase. No personal key is bundled or uploaded.
        & ssh-keygen -t rsa -b 3072 -f $key -C 'be6500panel-rescue'
        if ($LASTEXITCODE -ne 0) { throw 'SSH key generation failed.' }
    }
    if (-not (Test-Path -LiteralPath ($key + '.pub') -PathType Leaf)) { throw 'The matching .pub file is required.' }
    $null = Assert-OwnedPublicKey ($key + '.pub')
    return $key
}
function Get-PreviousManagerHash([string]$Work) {
    $archive = Join-Path $Work 'old-panel.tar.gz'
    $expected = [IO.File]::ReadAllText((Join-Path $Work 'old-panel.sha256')).Trim()
    if ($expected -notmatch '^[a-fA-F0-9]{64}$' -or
        (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected.ToLowerInvariant()) {
        throw 'Previous package checksum mismatch. Installation stopped before apply.'
    }
    $members = @(& tar -tzf $archive)
    if ($LASTEXITCODE -ne 0 -or @($members | Where-Object { $_ -ceq 'be6500-panel' }).Count -ne 1) {
        throw 'Previous package must contain one manager executable.'
    }
    $types = @(& tar -tvzf $archive 'be6500-panel')
    if ($LASTEXITCODE -ne 0 -or $types.Count -ne 1 -or -not $types[0].StartsWith('-')) {
        throw 'Previous manager must be a regular file.'
    }
    $out = Join-Path $Work 'old-manager'
    $null = New-Item -ItemType Directory -Path $out
    & tar -xzf $archive -C $out 'be6500-panel'
    if ($LASTEXITCODE -ne 0) { throw 'Unable to verify the previous manager.' }
    return (Get-FileHash -LiteralPath (Join-Path $out 'be6500-panel') -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Quote-NativeArgument([string]$Value) {
    # Windows CommandLineToArgvW quoting; harmless for PS7 Unix ProcessStartInfo too.
    return '"' + ([regex]::Replace(([regex]::Replace($Value, '(\\*)"', '$1$1\"')), '(\\+)$', '$1$1')) + '"'
}
function Send-PanelPassword([string]$Address, [string]$Key, [string]$Stage, [Security.SecureString]$Password, [int]$SshPort = 2222, [switch]$Interactive) {
    $plain = ConvertFrom-PrivateString $Password
    try {
        if ($plain.Length -lt 8 -or $plain.Length -gt 256 -or $plain -match '[\x00-\x1f\x7f]') { throw 'Use a panel password with 8 to 256 characters and no control characters.' }
        $arguments = @(Get-SshOptions $SshPort $Key -KeyOnly:(-not $Interactive)) + @(('root@' + $Address), ('umask 077; cat > ' + $Stage + '/panel-password; chmod 600 ' + $Stage + '/panel-password'))
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = (Get-Command ssh -ErrorAction Stop).Source
        $start.UseShellExecute = $false
        $start.RedirectStandardInput = $true
        $start.StandardInputEncoding = New-Object Text.UTF8Encoding($false)
        $start.Arguments = (($arguments | ForEach-Object { Quote-NativeArgument $_ }) -join ' ')
        $process = New-Object Diagnostics.Process
        $process.StartInfo = $start
        try {
            if (-not $process.Start()) { throw 'Unable to start SSH.' }
            $process.StandardInput.Write($plain)
            $process.StandardInput.Close()
            $process.WaitForExit()
            if ($process.ExitCode -ne 0) { throw 'Private password transfer failed.' }
        } finally { $process.Dispose() }
    } finally { $plain = $null }
}
function Invoke-PanelReleaseInstall([string]$Address, [int]$SshPort, [string]$Key,
    [string]$Repo, [string]$Tag, [string]$Name) {
    Assert-RouterAddress $Address
    Assert-ReleaseNames $Repo $Tag $Name
    foreach ($tool in @('ssh', 'scp', 'ssh-keygen', 'tar')) {
        if ($null -eq (Get-Command $tool -ErrorAction SilentlyContinue)) { throw ('Install the OpenSSH Client: missing ' + $tool) }
    }
    $assets = @('router-install.sh', 'router-rescue-setup.sh', 'rescue-bootstrap.sh', 'rescue-ssh.init')
    foreach ($file in $assets) {
        if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $file) -PathType Leaf)) { throw ('Download the complete installer tools: missing ' + $file) }
    }
    $rescueKey = Get-OwnedRescueKey $Key
    Write-Host 'If this key has a passphrase, load it into ssh-agent for the key-only rescue check.'
    $work = Join-Path ([IO.Path]::GetTempPath()) ('be6500panel-install-' + [Guid]::NewGuid().ToString('N'))
    $stage = '/tmp/be6500panel-install.' + [Guid]::NewGuid().ToString('N')
    $stageCreated = $false; $applied = $false; $backupReady = $false; $finished = $false
    $null = New-Item -ItemType Directory -Path $work
    try {
        # Upgrade backup contains packages only, never core/history/user credentials.
        if ($env:OS -eq 'Windows_NT') {
            $acl = New-Object Security.AccessControl.DirectorySecurity
            $acl.SetAccessRuleProtection($true, $false)
            $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
            $rule = New-Object Security.AccessControl.FileSystemAccessRule($sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
            $acl.AddAccessRule($rule)
            Set-Acl -LiteralPath $work -AclObject $acl
        } else {
            & chmod 700 $work
            if ($LASTEXITCODE -ne 0) { throw 'Unable to secure local temporary directory.' }
        }
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        $base = 'https://github.com/' + $Repo + '/releases/download/' + $Tag + '/'
        $archive = Join-Path $work $Name; $sums = Join-Path $work 'SHA256SUMS'
        Get-BoundedDownload ($base + $Name) $archive 67108864
        Get-BoundedDownload ($base + 'SHA256SUMS') $sums 262144
        $hash = Get-VerifiedReleaseHash $archive $sums $Name
        $hashFile = Join-Path $work 'panel.sha256'
        [IO.File]::WriteAllText($hashFile, $hash + "`n", (New-Object Text.UTF8Encoding($false)))
        $null = Invoke-InstallerSsh $Address $SshPort ('umask 077; mkdir ' + $stage + '; chmod 700 ' + $stage) $Key
        $stageCreated = $true
        Copy-ToRouter $Address $SshPort $archive ($stage + '/panel.tar.gz') $Key
        Copy-ToRouter $Address $SshPort $hashFile ($stage + '/panel.sha256') $Key
        foreach ($file in $assets) { Copy-ToRouter $Address $SshPort (Join-Path $PSScriptRoot $file) ($stage + '/' + $file) $Key }
        Copy-ToRouter $Address $SshPort ($rescueKey + '.pub') ($stage + '/rescue-authorized-key') $Key
        $initialMode = (Invoke-InstallerSsh $Address $SshPort 'if test -e /data/be6500panel; then printf upgrade; else printf fresh; fi' $Key).Trim()
        if ($initialMode -ceq 'fresh') {
            $password = Read-Host 'New panel login password (minimum 8 characters)' -AsSecureString
            try { Send-PanelPassword $Address $Key $stage $password $SshPort -Interactive } finally { $password.Dispose() }
        } elseif ($initialMode -cne 'upgrade') { throw 'Invalid initial install mode.' }
        $helper = 'sh ' + $stage + '/router-install.sh '
        $null = Invoke-InstallerSsh $Address $SshPort ($helper + 'prepare ' + $stage) $Key
        Write-Host 'Confirm the separate rescue port host key if asked. Keep this key for recovery.'
        # Establish trust interactively first, then prove key-only login from this host.
        $null = Invoke-InstallerSsh $Address 2222 'true' $rescueKey
        $null = Invoke-InstallerSsh $Address 2222 ('umask 077; printf verified > ' + $stage + '/rescue-verified') $rescueKey -KeyOnly
        $mode = (Invoke-InstallerSsh $Address $SshPort ('cat ' + $stage + '/mode') $Key).Trim()
        if ($mode -cne $initialMode) { throw 'Router installation mode changed during preparation.' }
        if ($mode -ceq 'upgrade') {
            foreach ($file in @('panel.tar.gz', 'panel.sha256', 'bootstrap.sh')) {
                Copy-FromRouter $Address $SshPort ('/data/be6500panel/' + $file) (Join-Path $work ('old-' + $file)) $Key
            }
            $oldManagerHash = Get-PreviousManagerHash $work
            $oldHashFile = Join-Path $work 'old-manager.sha256'
            [IO.File]::WriteAllText($oldHashFile, $oldManagerHash + "`n", (New-Object Text.UTF8Encoding($false)))
            Copy-ToRouter $Address $SshPort $oldHashFile ($stage + '/rollback/be6500-panel.sha256') $Key
            $backupReady = $true
        } elseif ($mode -cne 'fresh') { throw 'Invalid installer mode returned by router.' }
        $applied = $true
        $null = Invoke-InstallerSsh $Address $SshPort ($helper + 'apply ' + $stage) $Key
        $null = Invoke-InstallerSsh $Address $SshPort ($helper + 'verify ' + $stage) $Key
        $lan = (Invoke-InstallerSsh $Address $SshPort ('cat ' + $stage + '/lan-ip') $Key).Trim()
        Assert-RouterAddress $lan
        $finished = $true
        Write-Host ('Panel installation verified: http://' + $lan + ':8787')
        Write-Host ('Rescue SSH key login verified on port 2222. Key: ' + $rescueKey)
    } catch {
        if ($applied -and $backupReady) {
            try {
                $null = Invoke-InstallerSsh $Address $SshPort ('umask 077; mkdir -p ' + $stage + '/rollback') $Key
                foreach ($file in @('panel.tar.gz', 'panel.sha256', 'bootstrap.sh')) {
                    Copy-ToRouter $Address $SshPort (Join-Path $work ('old-' + $file)) ($stage + '/rollback/' + $file) $Key
                }
                $null = Invoke-InstallerSsh $Address $SshPort ($helper + 'rollback ' + $stage) $Key
                Write-Warning 'Installation failed. The previous panel package was restored.'
            } catch { Write-Warning ('Rollback could not be confirmed. Keep the local backup: ' + $work) }
        }
        throw
    } finally {
        if ($stageCreated) {
            try { $null = Invoke-InstallerSsh $Address $SshPort ('rm -rf ' + $stage) $Key }
            catch { Write-Warning 'Remote installer staging cleanup could not be confirmed.' }
        }
        # Retain the bounded manager-only backup after failed apply for manual rescue.
        if ($finished -or -not ($applied -and $backupReady)) { Remove-Item -LiteralPath $work -Recurse -Force }
    }
}
if ($MyInvocation.InvocationName -ne '.') {
    Invoke-PanelReleaseInstall $Router $Port $IdentityFile $Repository $Version $Asset
}
