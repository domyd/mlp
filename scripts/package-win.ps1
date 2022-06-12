$ffmpegBundleName = "ffmpeg-4.2.2-win64"
$ffmpegBundle = Join-Path $PSScriptRoot ".." "external" "ffmpeg" "$ffmpegBundleName.zip"
$dylibs = 
    "avcodec-58.dll",
    "avformat-58.dll",
    "avutil-56.dll",
    "swresample-3.dll"

# https://github.com/rust-lang/cargo/issues/4082#issuecomment-422507510
$pkgid = cargo pkgid
$pkgVersion = ($pkgid -split "#")[1]

Write-Host "Detected package version: $pkgVersion"

$guid = [guid]::NewGuid().ToString()
$workspacePath = Join-Path $env:TEMP "mlp.$guid"

function PrepareFFmpegDylibs() {
    New-Item -ItemType Directory -Force -Path $workspacePath | Out-Null
    Expand-Archive $ffmpegBundle -DestinationPath $workspacePath

    $releaseDir = Join-Path $workspacePath "mlp"
    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    
    foreach ($lib in $dylibs) {
        $libPath = Join-Path $workspacePath $ffmpegBundleName "bin" $lib
        Copy-Item -Path $libPath -Destination $releaseDir
    }
    Remove-Item -Force -Recurse (Join-Path $workspacePath $ffmpegBundleName)

    $releaseDir
}

try {
    Write-Host "Extracting FFmpeg binaries ..."
    $releaseDir = PrepareFFmpegDylibs

    Write-Host "Adding target/release/mlp.exe to archive ..."
    $releaseExe = Join-Path $PSScriptRoot ".." "target" "release" "mlp.exe"
    if (Test-Path $releaseExe) {
        Copy-Item $releaseExe $releaseDir
    } else {
        Write-Error "target/release/mlp.exe not found"
    }

    $zipFileName = "mlp-v$pkgVersion-win-x86_64.zip"
    $zipFile = Join-Path $releaseDir $zipFileName
    
    Write-Host "Compressing package to $zipFile ..."

    Push-Location $releaseDir
    Compress-Archive -Path ".\\*" -DestinationPath $zipFileName
    Pop-Location

    Copy-Item $zipFile $(Get-Location)
} finally {
    Remove-Item -Recurse -Force $workspacePath
}
