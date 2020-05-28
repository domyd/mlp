#!/bin/bash

realpath() {
    [[ $1 = /* ]] && echo "$1" || echo "$PWD/${1#./}"
}

ffmpegVersion="4.2.2"
ffmpegFile="ffmpeg-$ffmpegVersion-macos64-shared-lgpl"
ffmpegUrl="https://ffmpeg.zeranoe.com/builds/macos64/shared/$ffmpegFile.zip"
scriptDir="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
echo "$scriptDir"

declare -a dylibs=(
    "libavcodec.58.dylib" 
    "libavdevice.58.dylib" 
    "libavfilter.7.dylib"
    "libavformat.58.dylib" 
    "libavutil.56.dylib" 
    "libswresample.3.dylib"
    "libswscale.5.dylib")

# https://github.com/rust-lang/cargo/issues/4082#issuecomment-422507510
pkgVersion=$(cargo pkgid | cut -d# -f2 | cut -d: -f2)

echo "Detected package version: $pkgVersion"

uuid=$(uuidgen)

wsdir="$TMPDIR$uuid"
mkdir $wsdir && pushd $wsdir
curl $ffmpegUrl --output "$ffmpegFile.zip"
tar xzf "$ffmpegFile.zip"

for lib in ${dylibs[@]}
do
    cp "$ffmpegFile/bin/$lib" .
done

rm -rf $ffmpegFile
rm "$ffmpegFile.zip"

cp "$scriptDir/../target/release/mlp" .

popd

tar -czvf "mlp-v$pkgVersion-macos-x86_64.tar.gz" -C $wsdir .
