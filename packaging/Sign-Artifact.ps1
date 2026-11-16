param(
    [Parameter(Mandatory)][string]$Path
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($env:SAKURA_SIGNING_PFX)) {
    Write-Output 'Signing certificate is unavailable. Artifact is unsigned.'
    exit 0
}

$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$signTool = Get-ChildItem -LiteralPath $sdk -Recurse -Filter signtool.exe -File |
    Where-Object { $_.Directory.Name -eq 'x64' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (!$signTool) { throw 'Windows SDK SignTool was not found.' }

$cert = Join-Path ([IO.Path]::GetTempPath()) ('sakura-signing-' + [guid]::NewGuid() + '.pfx')
try {
    [IO.File]::WriteAllBytes($cert, [Convert]::FromBase64String($env:SAKURA_SIGNING_PFX))
    & $signTool sign /f $cert /p $env:SAKURA_SIGNING_PASSWORD /fd SHA256 /tr 'http://timestamp.digicert.com' /td SHA256 $Path
    if ($LASTEXITCODE -ne 0) { throw 'Authenticode signature failed.' }
    & $signTool verify /pa $Path
    if ($LASTEXITCODE -ne 0) { throw 'Authenticode signature verification failed.' }
} finally {
    if (Test-Path -LiteralPath $cert) { Remove-Item -LiteralPath $cert }
}
