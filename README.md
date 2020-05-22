# mlp

A Dolby TrueHD demuxer and utility tool, with a focus on accurately and correctly demuxing a TrueHD stream from a decrypted blu-ray disc.

Dual-licensed under MIT and Apache 2.0.

## Install

You can download the latest binaries from the release page of this repository.

## Usage

Demux the TrueHD stream from a given blu-ray playlist file, optionally with the given angle (which starts at 1):

```powershell
PS> mlp demux playlist "F:\BDMV\PLAYLIST\00800.mpls" --output "out.thd" --angle 2
```

Print the segment map and any available angles for the given playlist file:

```powershell
PS> mlp demux playlist "F:\BDMV\PLAYLIST\00800.mpls"
```

Demux the TrueHD stream from a list of stream files in the `F:\BDMV\STREAM` directory, and save it to `out.thd`. The files are chosen based either a comma-separated list of numbers or `+`-separated list of file names:

```powershell
PS> mlp demux segments -s "F:\BDMV\STREAM" -o "out.thd" -l "55,56"
PS> mlp demux segments -s "F:\BDMV\STREAM" -o "out.thd" --segment-files "00055.m2ts+00056.m2ts"
```

Show frame count and duration information of a TrueHD stream:

```powershell
PS> mlp info "out.thd"
PS> mlp info "00055.m2ts"
```

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

- [x] Blu-ray playlist support
- [ ] Better console/log output
- [ ] Better/more relevant stats at the end of demuxing
- [ ] Performance optimization
- [x] Improve `info` command, support .m2ts files
- [x] Better CLI help output
- [ ] More tests
- [ ] Support Linux and macOS
