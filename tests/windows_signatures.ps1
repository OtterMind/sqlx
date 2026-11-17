$ErrorActionPreference = 'Stop'
$verify = Join-Path $PSScriptRoot '../scripts/verify-windows-signatures.ps1'
$directory = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
$fixture = Join-Path $PSHOME 'pwsh.exe'
$signature = Get-AuthenticodeSignature -LiteralPath $fixture
if ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate) {
    throw 'The PowerShell fixture must have a valid timestamped signature.'
}
$fingerprint = $signature.SignerCertificate.GetCertHashString(
    [System.Security.Cryptography.HashAlgorithmName]::SHA256
)
$names = @(python -c "import sys; sys.path.insert(0, 'scripts'); import package; print('\n'.join(name + '.exe' for name in package.BINARIES.values()))")
if ($LASTEXITCODE -ne 0) { throw 'Could not read the packaging executable list.' }

function Assert-Rejected([string]$ExpectedMessage, [string]$Certificate = $fingerprint) {
    try {
        & $verify -Directory $directory -CertificateSha256 $Certificate
    } catch {
        if ($_.Exception.Message -notlike "*$ExpectedMessage*") { throw }
        return
    }
    throw "Verification unexpectedly passed: $ExpectedMessage"
}

try {
    New-Item -ItemType Directory -Path $directory | Out-Null
    foreach ($name in $names) { Copy-Item -LiteralPath $fixture -Destination (Join-Path $directory $name) }
    & $verify -Directory $directory -CertificateSha256 $fingerprint
    Assert-Rejected 'Unexpected signing certificate' ('0' * 64)

    $extra = Join-Path $directory 'extra.exe'
    Copy-Item -LiteralPath $fixture -Destination $extra
    Assert-Rejected 'Expected exactly'
    Remove-Item -LiteralPath $extra

    $target = Join-Path $directory $names[0]
    $bytes = [System.IO.File]::ReadAllBytes($target)
    $bytes[1024] = $bytes[1024] -bxor 1
    [System.IO.File]::WriteAllBytes($target, $bytes)
    Assert-Rejected 'Invalid signature'

    [System.IO.File]::WriteAllText($target, 'unsigned executable')
    Assert-Rejected 'Invalid signature'
    Remove-Item -LiteralPath $target
    Assert-Rejected 'Expected exactly'
    Write-Host 'Windows signature verification checks passed.'
} finally {
    Remove-Item -LiteralPath $directory -Recurse -Force
}
