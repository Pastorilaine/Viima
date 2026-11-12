param(
    [string]$DownloadBaseUrl = "",
    [string]$ReleaseNotes = "",
    [switch]$SkipChecks
)

$ErrorActionPreference = "Stop"

$LowerAe = [char]0x00E4
$LowerOe = [char]0x00F6
$UpperAe = [char]0x00C4
$UpperOe = [char]0x00D6

if ([string]::IsNullOrWhiteSpace($ReleaseNotes)) {
    $ReleaseNotes = "Viima-selaimen p${LowerAe}ivitys. Muutokset: https://github.com/Pastorilaine/Viima/releases"
}

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Push-Location $RootDir

try {
    $CargoToml = Get-Content -Path "Cargo.toml" -Raw -Encoding UTF8
    if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    } else {
        throw "Ei voitu lukea versionumeroa tiedostosta Cargo.toml"
    }

    if ([string]::IsNullOrWhiteSpace($DownloadBaseUrl)) {
        $DownloadBaseUrl = "https://github.com/Pastorilaine/Viima/releases/download/v$Version"
    }
    $DownloadBaseUrl = $DownloadBaseUrl.TrimEnd('/')

    Write-Host "=== Viima v$Version julkaisuk${LowerAe}nn${LowerOe}s ===" -ForegroundColor Cyan

    if (-not $SkipChecks) {
        Write-Host "[1/5] Ajetaan testit ja clippy..." -ForegroundColor Yellow
        cargo test
        if ($LASTEXITCODE -ne 0) { throw "cargo test ep${LowerAe}onnistui." }

        cargo clippy --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw "cargo clippy ep${LowerAe}onnistui." }
    } else {
        Write-Host "[1/5] Ohitetaan testit (-SkipChecks)." -ForegroundColor DarkGray
    }

    Write-Host "[2/5] K${LowerAe}${LowerAe}nnet${LowerAe}${LowerAe}n Viima.exe (release)..." -ForegroundColor Yellow
    $env:VIIMA_PAYLOAD = ""
    $env:OMA_SELAIN_PAYLOAD = ""
    cargo build --release --bin viima
    if ($LASTEXITCODE -ne 0) { throw "Selaimen release-k${LowerAe}${LowerAe}nn${LowerOe}s ep${LowerAe}onnistui." }

    $BrowserExe = (Resolve-Path "target\release\viima.exe").Path

    Write-Host "[3/5] K${LowerAe}${LowerAe}nnet${LowerAe}${LowerAe}n Viima-Setup.exe upotetulla selaimella..." -ForegroundColor Yellow
    $env:VIIMA_PAYLOAD = $BrowserExe
    $env:OMA_SELAIN_PAYLOAD = $BrowserExe
    cargo build --release --bin viima_setup
    if ($LASTEXITCODE -ne 0) { throw "Asennusohjelman release-k${LowerAe}${LowerAe}nn${LowerOe}s ep${LowerAe}onnistui." }

    $SetupExe = (Resolve-Path "target\release\viima_setup.exe").Path

    Write-Host "[4/5] Kootaan julkaisupaketti kansioihin releases\v$Version\ ja releases\latest\..." -ForegroundColor Yellow
    $ReleasesDir = Join-Path $RootDir "releases"
    $VersionDir = Join-Path $ReleasesDir "v$Version"
    $LatestDir = Join-Path $ReleasesDir "latest"
    $PortableStage = Join-Path $ReleasesDir "_portable_tmp"

    Get-Process -Name "Viima*", "OmaSelain*" -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$ReleasesDir*" } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300

    foreach ($d in @($PortableStage, $VersionDir, $LatestDir)) {
        if (Test-Path $d) { Remove-Item -Path $d -Recurse -Force -ErrorAction SilentlyContinue }
        New-Item -ItemType Directory -Path $d -Force | Out-Null
    }

    $ZipName = "Viima_v${Version}_Windows_x64.zip"
    $VerBrowserExe = Join-Path $VersionDir "Viima.exe"
    $VerSetupExe = Join-Path $VersionDir "Viima-Setup.exe"
    $VerZipPath = Join-Path $VersionDir $ZipName
    $VerManifestPath = Join-Path $VersionDir "latest.json"

    Copy-Item -Path $BrowserExe -Destination $VerBrowserExe -Force
    Copy-Item -Path $SetupExe -Destination $VerSetupExe -Force

    # Portable zip contents
    Copy-Item -Path $BrowserExe -Destination (Join-Path $PortableStage "Viima.exe") -Force
    $BatContent = "@echo off`r`nstart `"`" `"%~dp0Viima.exe`"`r`n"
    [System.IO.File]::WriteAllText((Join-Path $PortableStage "Kaynnista_Viima.bat"), $BatContent, [System.Text.Encoding]::ASCII)

    $InfoText = @"
Viima v$Version (Portable Windows x64) - IT-Veljekset Group
==================================================================

K${UpperAe}YTT${UpperOe}:
- Kaksoisnapsauta Viima.exe tai Kaynnista_Viima.bat.
- Suosittelemme varsinaiseen k${LowerAe}ytt${LowerOe}${LowerOe}n Viima-Setup.exe -asennusohjelmaa,
  joka asentaa Viima-selaimen kansioon %LOCALAPPDATA%\IT-Veljekset\Viima,
  luo Viima-pikakuvakkeet ja aktivoi automaattiset p${LowerAe}ivitykset.

VAATIMUKSET:
- Windows 10 / 11 (64-bit) ja Microsoft Edge WebView2 Runtime.
"@
    [System.IO.File]::WriteAllText((Join-Path $PortableStage "OHJE_JA_INFO.txt"), $InfoText, (New-Object System.Text.UTF8Encoding($false)))

    Compress-Archive -Path (Join-Path $PortableStage "*") -DestinationPath $VerZipPath -Force
    Remove-Item -Path $PortableStage -Recurse -Force

    Write-Host "[5/5] Luodaan latest.json ja peilataan releases\latest\..." -ForegroundColor Yellow
    $Sha256 = (Get-FileHash -Path $VerBrowserExe -Algorithm SHA256).Hash.ToLowerInvariant()

    $ManifestObj = [ordered]@{
        version       = $Version
        download_url  = "$DownloadBaseUrl/Viima.exe"
        sha256        = $Sha256
        release_notes = $ReleaseNotes
    }
    $ManifestJson = $ManifestObj | ConvertTo-Json -Depth 4
    [System.IO.File]::WriteAllText($VerManifestPath, $ManifestJson + "`n", (New-Object System.Text.UTF8Encoding($false)))

    Copy-Item -Path (Join-Path $VersionDir "*") -Destination $LatestDir -Force

    Write-Host ""
    Write-Host "Valmis! Julkaisutiedostot kansioissa releases\latest\ ja releases\v$Version\:" -ForegroundColor Green
    Get-ChildItem -Path $LatestDir | Select-Object Name, @{Name="MB";Expression={[math]::Round($_.Length / 1MB, 2)}} | Format-Table -AutoSize
    Write-Host "Viima.exe SHA-256: $Sha256" -ForegroundColor Cyan
    Write-Host "Lataa GitHub Release v$Version -julkaisuun kaikki 4 tiedostoa kansiosta releases\latest\:"
    Write-Host "  - Viima-Setup.exe"
    Write-Host "  - Viima.exe"
    Write-Host "  - latest.json"
    Write-Host "  - $ZipName"
}
finally {
    Remove-Item Env:\VIIMA_PAYLOAD -ErrorAction SilentlyContinue
    Remove-Item Env:\OMA_SELAIN_PAYLOAD -ErrorAction SilentlyContinue
    Pop-Location
}
