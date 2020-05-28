$ffmpegVersion = "4.2.2"
$ffmpegFile = "ffmpeg-$ffmpegVersion-win64-shared-lgpl"
$ffmpegUrl = "https://ffmpeg.zeranoe.com/builds/win64/shared/$ffmpegFile.zip"

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
    $dlPath = Join-Path $workspacePath "ffmpeg.zip"

    Invoke-WebRequest $ffmpegUrl -OutFile $dlPath

    Expand-Archive $dlPath -DestinationPath $workspacePath
    Remove-Item $dlPath

    $releaseDir = Join-Path $workspacePath "mlp"
    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    
    foreach ($lib in $dylibs) {
        $libPath = Join-Path $workspacePath $ffmpegFile "bin" $lib
        Copy-Item -Path $libPath -Destination $releaseDir
    }
    Remove-Item -Force -Recurse (Join-Path $workspacePath $ffmpegFile)

    $releaseDir
}

try {
    Write-Host "Downloading and extracting FFmpeg binaries ..."
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
    7z a $zipFileName *
    Pop-Location

    Copy-Item $zipFile $(Get-Location)
} finally {
    Remove-Item -Recurse -Force $workspacePath
}
