param([string]$Version = $env:SQLX_VERSION)
$ErrorActionPreference = 'Stop'
$releaseBase = if ($env:SQLX_RELEASE_BASE) { $env:SQLX_RELEASE_BASE } else { 'https://github.com/OtterMind/sqlx/releases' }
$installDir = if ($env:SQLX_INSTALL_DIR) { $env:SQLX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\SQLX' }
if ([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') { throw 'Windows x64 is the supported Windows target.' }
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('sqlx-install-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    if (-not $Version) {
        try { $Version = (Invoke-RestMethod "$releaseBase/latest/download/release-version.txt").Trim() }
        catch { throw 'No downloadable release is available. See the README source-install instructions.' }
    }
    if ($Version -notmatch '^[0-9][0-9A-Za-z.+-]*$') { throw 'Invalid release version.' }
    $asset = 'sqlx-windows-x64.zip'
    $archive = Join-Path $temporary $asset
    Invoke-WebRequest "$releaseBase/download/v$Version/$asset" -OutFile $archive
    $checksums = (Invoke-WebRequest "$releaseBase/download/v$Version/SHA256SUMS").Content
    $expected = ($checksums -split "`n" | Where-Object { $_.Trim() -match ('^[a-fA-F0-9]{64}\s+' + [regex]::Escape($asset) + '$') }) -replace '\s+.*$', ''
    if (-not $expected -or (Get-FileHash $archive -Algorithm SHA256).Hash -ine $expected.Trim()) { throw 'Download checksum verification failed.' }
    $unpacked = Join-Path $temporary 'unpacked'
    Expand-Archive $archive -DestinationPath $unpacked
    $binary = Join-Path $unpacked 'sqlx.exe'
    $actualVersion = & $binary --version
    if ($LASTEXITCODE -ne 0 -or $actualVersion -notmatch '\(OtterMind/sqlx\)') { throw 'Archive is not an OtterMind SQLX executable.' }
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    $destination = Join-Path $installDir 'sqlx.exe'
    if (Test-Path $destination) {
        if ((Get-Item $destination).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Existing sqlx is a link; choose another SQLX_INSTALL_DIR.' }
        try { $existing = & $destination --version } catch { $existing = '' }
        if ($existing -notmatch '\(OtterMind/sqlx\)') { throw 'Another sqlx executable exists; choose another SQLX_INSTALL_DIR.' }
    }
    $staged = Join-Path $installDir ('.sqlx-install-' + [guid]::NewGuid() + '.exe')
    Copy-Item $binary $staged
    Move-Item -Force $staged $destination
    Write-Output "Installed $actualVersion to $destination"
    Write-Output "Add $installDir to PATH, then run: sqlx init"
} finally { Remove-Item -Recurse -Force $temporary }
