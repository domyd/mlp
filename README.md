# mlp

A Dolby TrueHD utility tool.

## Features
* Append blu-ray TrueHD streams together into one TrueHD stream.
* Print information about a TrueHD stream.

## Build
Probably only works on Windows. Downloads the ffmpeg 4.2.2 binaries and library files from the internet during the build phase.

From the repository root directory, run:
```powershell
$env:INCLUDE="$(Get-Location)\external\ffmpeg\include"
cargo run
```
