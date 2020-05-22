# mlp

A Dolby TrueHD demuxer and utility tool, with a focus on accurately and correctly demuxing a TrueHD stream from a decrypted blu-ray disc.

Dual-licensed under MIT and Apache 2.0.

## Install
You can download the latest binaries from the release page of this repository.

## Usage
Demux the TrueHD stream from a given blu-ray playlist file, optionally with the given `<ANGLE>`, and save it to `<OUT-FILE>`:
```powershell
mlp demux playlist [<ANGLE>] <PLAYLIST-FILE> <OUT-FILE>
# Example:
# mlp demux playlist "F:\BDMV\PLAYLIST\00800.mpls" out.thd
# mlp demux playlist --angle 2 "F:\BDMV\PLAYLIST\00800.mpls" out.thd
```
> NOTE: The angle index is 1-based.

Demux the TrueHD stream from a list of `.m2ts` files, which are located in `<STEAM-DIR>`, and save it to `<OUT-FILE>`. The segments are given either by a comma-separated `<SEGMENT-MAP>`, or a `+`-separated list of `.m2ts` file names:
```powershell
mlp demux segments -s <STREAM-DIR> -o <OUT-FILE> -l <SEGMENT-MAP>
mlp demux segments -s <STREAM-DIR> -o <OUT-FILE> --segment-files <SEGMENT-FILES>
# Examples:
# mlp demux segments -s "F:\BDMV\STREAM\" -o out.thd -l 55,56
# mlp demux segments -s "F:\BDMV\STREAM\" -o out.thd --segment-files "00055.m2ts+00056.m2ts"
```

Show frame count and duration information of a truehd stream:
```powershell
mlp info <TRUEHD-FILE>
# Example:
# mlp info out.thd
```
> NOTE: This doesn't currently work for .m2ts files. You must supply a demuxed TrueHD stream.

## FAQ
### Aren't there already other demuxing tools out there?
Absolutely. However, all of them fail in [different](https://www.makemkv.com/forum/viewtopic.php?f=6&t=21513&p=84453#p84453) [ways](http://rationalqm.us/board/viewtopic.php?p=10841#p10841) on TrueHD streams, especially on discs that contain a large number of segments, which has resulted in desync and noticeable audio artifacts. This tool aims to be a perfectly accurate TrueHD demuxer that doesn't produce invalid, broken, or out-of-sync streams.

## Build
Currently building is tested and supported only on Windows. Other platforms soon to follow!

From the repository root directory, run:
```powershell
$env:INCLUDE="$(Get-Location)\external\ffmpeg\include"
cargo run
```

> NOTE: Downloads the ffmpeg 4.2.2 LGPL binaries and library files from the internet during the build phase.

## TODO list

- [X] Blu-ray playlist support
- [ ] Better console/log output
- [ ] Better/more relevant stats at the end of demuxing
- [ ] Performance optimization
- [ ] Improve `info` command, support .m2ts files
- [ ] Better CLI help output
- [ ] More tests
- [ ] Support Linux and macOS
