$candidates = @(
    (Join-Path $PSScriptRoot 'releases\latest\OmaSelain.exe'),
    (Join-Path $PSScriptRoot 'target\release\oma_selain.exe')
)
$browserPath = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $browserPath) {
    throw "Viima-selainta ei loydy. Aja ensin .\scripts\build_release.ps1."
}
Start-Process -FilePath $browserPath
