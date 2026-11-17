param(
    [Parameter(Mandatory)]
    [string]$Directory,
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Fa-f0-9]{64}$')]
    [string]$CertificateSha256
)

$ErrorActionPreference = 'Stop'
$expected = @(
    'sqlx.exe', 'sqlx-driver-mysql.exe', 'sqlx-driver-postgres.exe',
    'sqlx-driver-redis.exe', 'sqlx-driver-mongodb.exe', 'sqlx-driver-sqlite.exe',
    'sqlx-driver-duckdb.exe', 'sqlx-ui.exe'
)
$files = @(Get-ChildItem -LiteralPath $Directory -Force)
if ($files.Count -ne $expected.Count -or @($files | Where-Object {
    $_.Name -notin $expected -or $_.PSIsContainer -or
    ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -or $_.Length -eq 0
}).Count -gt 0) {
    throw 'Expected exactly the eight SQLX executable files.'
}

foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "Invalid signature for $($file.Name): $($signature.Status) $($signature.StatusMessage)"
    }
    $fingerprint = $signature.SignerCertificate.GetCertHashString(
        [System.Security.Cryptography.HashAlgorithmName]::SHA256
    )
    if ($fingerprint -ne $CertificateSha256) {
        throw "Unexpected signing certificate for $($file.Name): $fingerprint"
    }
    if (-not $signature.TimeStamperCertificate) {
        throw "Missing trusted timestamp for $($file.Name)."
    }
    Write-Host "Verified $($file.Name), certificate SHA256 $fingerprint"
}
