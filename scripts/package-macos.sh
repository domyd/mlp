#!/bin/bash

realpath() {
    [[ $1 = /* ]] && echo "$1" || echo "$PWD/${1#./}"
}

ffmpegBundleName="ffmpeg-4.2.2-macos64.zip"
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
cp "$scriptDir/../external/ffmpeg/$ffmpegBundleName.zip" .
tar xzf "$ffmpegBundleName.zip"

for lib in ${dylibs[@]}
do
    cp "$ffmpegBundleName/bin/$lib" .
done

rm -rf $ffmpegBundleName
rm "$ffmpegBundleName.zip"

cp "$scriptDir/../target/release/mlp" .

popd

tar -czvf "mlp-v$pkgVersion-macos-x86_64.tar.gz" -C $wsdir .
