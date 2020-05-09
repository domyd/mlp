# mlp

A Dolby TrueHD utility tool.

## Features
* Append blu-ray TrueHD streams together into one TrueHD stream.
* Print information about a TrueHD stream.

## Build
Only tested on Windows.

Download the `dev` and `shared` builds for ffmpeg 4.2.2, which contain the required header files and dynamic libraries: 
https://ffmpeg.zeranoe.com/builds/win64/dev/ffmpeg-4.2.2-win64-dev.zip
https://ffmpeg.zeranoe.com/builds/win64/shared/ffmpeg-4.2.2-win64-shared.zip

Set the `INCLUDE` environment variable:
```
$env:INCLUDE="<dev-build-dir>\include"
```

Set the linker dir in `RUSTFLAGS`:
```
$env:RUSTFLAGS='-L <dev-build-dir>\lib'
```

Add the DLL dir to `PATH`:
```
$env:PATH += ";<shared-build-dir>\bin"
```

Build:
```
cargo build
```
