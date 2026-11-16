param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Output
)

$ErrorActionPreference = 'Stop'
if (!$env:SAKURA_RELEASE_PRIVATE_KEY -or !$env:SAKURA_RELEASE_PUBLIC_KEY) {
    Write-Output 'Release feed keys are unavailable.'
    exit 0
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid update version.' }
if ((Get-AuthenticodeSignature -LiteralPath $Installer).Status -ne 'Valid') {
    throw 'Only an Authenticode-signed MSI can be published in the update feed.'
}

$openssl = (Get-Command openssl.exe -ErrorAction SilentlyContinue).Source
if (!$openssl) { $openssl = Join-Path $env:ProgramFiles 'Git/usr/bin/openssl.exe' }
if (!(Test-Path -LiteralPath $openssl)) { throw 'OpenSSL was not found.' }
$prefix = Join-Path ([IO.Path]::GetTempPath()) ('sakura-update-' + [guid]::NewGuid())
$private = "$prefix.pem"
$public = "$prefix.der"
$payloadFile = "$prefix.json"
$signature = "$prefix.sig"
try {
    [IO.File]::WriteAllBytes($private, [Convert]::FromBase64String($env:SAKURA_RELEASE_PRIVATE_KEY))
    & $openssl pkey -in $private -pubout -outform DER -out $public
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read Ed25519 release key.' }
    $der = [IO.File]::ReadAllBytes($public)
    if ($der.Length -ne 44 -or [Convert]::ToHexString($der[0..11]) -ne '302A300506032B6570032100' -or
        [Convert]::ToHexString($der[12..43]) -ne $env:SAKURA_RELEASE_PUBLIC_KEY.Trim()) {
        throw 'Release key does not match the application verifier.'
    }
    $hash = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
    $payload = @{ version = $Version; sha256 = $hash } | ConvertTo-Json -Compress
    [IO.File]::WriteAllText($payloadFile, $payload)
    & $openssl pkeyutl -sign -rawin -inkey $private -in $payloadFile -out $signature
    if ($LASTEXITCODE -ne 0) { throw 'Cannot sign release manifest.' }
    $signed = @{ payload = $payload; signature = [Convert]::ToHexString([IO.File]::ReadAllBytes($signature)).ToLowerInvariant() }
    $signed | ConvertTo-Json -Compress | Set-Content -LiteralPath $Output -Encoding utf8NoBOM
} finally {
    foreach ($path in $private,$public,$payloadFile,$signature) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    }
}
