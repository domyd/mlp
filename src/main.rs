use clap::{App, Arg, ArgGroup, ArgSettings};
use itertools::Itertools;
use libav::DemuxOptions;
use log::*;
use mlp::{MlpFrame, MlpFrameReader, MlpIterator};
use num_format::{Locale, ToFormattedString};
use simplelog::*;
use std::fs::File;
use std::{
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};
pub mod libav;
pub mod mlp;

fn main() -> std::io::Result<()> {
    let args = App::new("TrueHD Demuxer")
        .version("0.1")
        .author("Dominik Mydlil <dominik.mydlil@outlook.com>")
        .about("Dolby TrueHD utility tool")
        .subcommand(
            App::new("demux")
                .about("Demux THD stream from blu-ray files.")
                .subcommand(
                    App::new("playlist")
                        .about("Demux from a blu-ray playlist file.")
                        .arg(
                            Arg::with_name("playlist")
                                .about("Sets the path to the playlist file (.mpls).")
                                .value_name("PLAYLIST-FILE")
                                .required(true),
                        )
                        .arg(
                            Arg::with_name("output")
                                .about("Sets the output TrueHD file.")
                                .value_name("OUTPUT-FILE")
                                .required(true),
                        ),
                )
                .subcommand(
                    App::new("segments")
                        .about("Demux from blu-ray media files.")
                        .arg(
                            Arg::with_name("segment-list")
                                .about("Sets the comma-separated list of TrueHD segments.")
                                .short('l')
                                .long("segment-list")
                                .group("segment-list-group")
                                .takes_value(true)
                                .min_values(2)
                                .value_delimiter(",")
                                .value_name("SEGMENT-LIST"),
                        )
                        .arg(
                            Arg::with_name("dgdemux-segment-list")
                                .about("Sets the list of segments, in DGDemux's format.")
                                .long("dgdemux-segment-list")
                                .value_name("SEGMENT-LIST")
                                .group("segment-list-group")
                                .takes_value(true),
                        )
                        .arg(
                            Arg::with_name("stream-dir")
                                .about("Sets the directory that contains the m2ts source files.")
                                .requires("segment-list-group")
                                .value_name("DIRECTORY")
                                .short('s')
                                .long("stream-dir"),
                        )
                        .arg(
                            Arg::with_name("output")
                                .about("Sets the output TrueHD file.")
                                .short('o')
                                .long("output")
                                .value_name("OUTPUT-FILE")
                                .required(true),
                        )
                        .group(
                            ArgGroup::with_name("segment-list-group")
                                .requires("stream-dir")
                                .required(true),
                        ),
                )
                .arg(
                    Arg::with_name("threshold")
                        .long("threshold")
                        .value_name("THRESHOLD")
                        .default_value("1024")
                        .validator(|s| {
                            s.parse::<i32>().map_err(|_| String::from("Must be a number."))
                        })
                        .global(true)
                        .about("Determines the sensitivity of the TrueHD frame comparer. Defaults to 1024.")
                        .long_about(
"This value determines when two samples can be considered equal 
for the purposes of checking whether two TrueHD frames contain 
the same audio. The default value of 1024 means that two frames

    frame 1: [-250282, -231928, -200182, ...]
    frame 2: [-249876, -232048, -201012, ...]

would be considered equal. This should practically never lead 
to false positives, as the deltas will be orders of magnitude 
higher (in the hundreds of thousands range) in case two frames 
contain different audio.
"
                        ),
                ),
        )
        .subcommand(
            App::new("info")
                .about("Prints information about the TrueHD stream.")
                .arg(Arg::with_name("stream").value_name("STREAM").required(true)),
        )
        .subcommand(
            App::new("ff")
                .arg("-i, --input <FILE>")
                .arg("-h, --head 'print first frame'")
                .arg("-t, --tail 'print last frame'")
        )
        .arg(
            Arg::with_name("verbosity")
                .about("Sets the output verbosity.")
                .global(true)
                .setting(ArgSettings::MultipleOccurrences)
                .short('v')
        )
        .arg(
            Arg::with_name("ffmpeg-log")
                .about("Enable FFmpeg log output.")
                .long_about(
"The verbosity of the FFmpeg log output will be determined from the -v flag: 
    not set (0): info
    -v      (1): verbose
    -vv     (2): debug
    -vvv    (3): trace
")
                .global(true)
                .takes_value(false)
                .long("enable-ffmpeg-log")
        )
        .get_matches();

    let verbosity_level = args.occurrences_of("verbosity").min(3);
    let verbosity = match verbosity_level {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        2 => LevelFilter::Trace,
        _ => LevelFilter::Off,
    };

    let ffmpeg_log_level = match verbosity_level {
        0 => 32,
        1 => 40,
        2 => 48,
        3 => 56,
        _ => 32,
    };

    let logger_config = {
        let mut builder = ConfigBuilder::new();
        builder
            .set_thread_level(LevelFilter::Off)
            .set_target_level(LevelFilter::Off)
            .set_time_to_local(true);
        if !args.is_present("ffmpeg-log") {
            builder.add_filter_ignore_str("ffmpeg");
        }
        builder.build()
    };
    TermLogger::init(verbosity, logger_config, TerminalMode::Mixed).unwrap();
    libav::av_log::configure_rust_log(ffmpeg_log_level);

    match args.subcommand() {
        ("demux", Some(sub)) => match sub.subcommand() {
            ("playlist", Some(_)) => {
                println!("Not yet implemented, sorry!");
                Ok(())
            }
            ("segments", Some(sub)) => {
                let out_file_str: &str = sub.value_of("output").unwrap();
                let out_file = File::create(out_file_str)
                    .expect(format!("Failed to create file '{0}'.", out_file_str).as_ref());

                let src_dir_str: &str = sub.value_of("stream-dir").unwrap();
                let src_dir_buf = PathBuf::from(src_dir_str);

                let threshold = sub
                    .value_of("threshold")
                    .map(|s| s.parse::<i32>().unwrap())
                    .unwrap();

                let segments: Option<Vec<PathBuf>> = {
                    if let Some(values) = sub.values_of("segment-list") {
                        let values: Result<Vec<u16>, _> =
                            values.map(|s| s.parse::<u16>()).collect();
                        let vn: Vec<u16> = values.unwrap();
                        Some(vn)
                    } else if let Some(dg_list) = sub.value_of("dgdemux-segment-list") {
                        let values: Vec<u16> = dgdemux::parse_segment_string(dg_list).unwrap();
                        Some(values)
                    } else {
                        // can't happen, clap makes sure of that
                        None
                    }
                }
                .map(|s| {
                    s.iter()
                        .map(|x| get_path_for_segment(*x, src_dir_buf.as_path()))
                        .collect()
                });

                if let Some(segment_paths) = segments {
                    let segment_paths: Vec<&Path> =
                        segment_paths.iter().map(|p| p.as_path()).collect();

                    {
                        let mut writer = BufWriter::new(out_file);
                        let demux_opts = DemuxOptions {
                            audio_match_threshold: threshold,
                        };
                        let _overrun =
                            libav::demux::demux_thd(&segment_paths, &demux_opts, &mut writer)
                                .unwrap();
                    }

                    print_stream_info(PathBuf::from(&out_file_str).as_path())?;
                } else {
                    error!("segment list invalid.");
                }

                Ok(())
            }
            _ => Ok(()),
        },
        ("info", Some(sub)) => {
            let path = PathBuf::from(sub.value_of("stream").unwrap());
            print_stream_info(path.as_path())?;
            Ok(())
        }
        ("ff", Some(sub)) => {
            let input_path = PathBuf::from(sub.value_of("input").unwrap());
            let threshold = sub
                .value_of("threshold")
                .map(|s| s.parse::<i32>().unwrap())
                .unwrap();
            if let Some(head) = match sub.is_present("head") {
                true => Some(true),
                false => match sub.is_present("tail") {
                    true => Some(false),
                    false => None,
                },
            } {
                debug!("head: {}", head);
                libav::thd_audio_read_test(&input_path, head, threshold).unwrap();
            } else {
                error!("wrong args");
            }

            Ok(())
        }
        _ => Ok(()),
    }
}

mod dgdemux {
    pub fn parse_segment_string(list_str: &str) -> Result<Vec<u16>, std::num::ParseIntError> {
        list_str
            .split('+')
            .map(|x| x.replace(".m2ts", "").parse::<u16>())
            .collect()
    }

    #[test]
    fn dgdemux_segment_map_test() {
        let dgd = "00055.m2ts+00056.m2ts+00086.m2ts+00058.m2ts+00074.m2ts";
        let expect: Vec<u16> = vec![55, 56, 86, 58, 74];
        assert_eq!(expect, parse_segment_string(dgd).unwrap());
    }
}

fn print_stream_info(filepath: &Path) -> std::io::Result<()> {
    let file = File::open(filepath)?;

    info!("Counting output file frames ...");

    let mut num_frames = 0;
    let mut num_major_frames = 0;
    for frame in MlpIterator::new(BufReader::new(file)) {
        if frame.has_major_sync {
            num_major_frames += 1;
        }
        num_frames += 1;
    }

    let duration = (num_frames * 40) as f64 / 48000_f64;

    info!("Assuming 48 KHz sampling frequency and 40 samples per frame.");
    info!(
        "Total MLP frame count: {:>14}",
        num_frames.to_formatted_string(&Locale::en)
    );
    info!(
        "  Major frames: {:>21}",
        num_major_frames.to_formatted_string(&Locale::en)
    );
    info!(
        "  Minor frames: {:>21}",
        (num_frames - num_major_frames).to_formatted_string(&Locale::en)
    );
    info!(
        "Number of audio samples: {:>12}",
        (num_frames * 40).to_formatted_string(&Locale::en)
    );
    info!("Duration: {:>35.7} seconds", duration);

    Ok(())
}

#[allow(dead_code)]
fn write_mlp_frames<W: Write>(
    frames: &[MlpFrame],
    src_dir: &Path,
    writer: &mut W,
) -> std::io::Result<()> {
    let mut frames_by_segment: Vec<(u16, Vec<&MlpFrame>)> = Vec::new();
    for (key, group) in &frames.into_iter().group_by(|f| f.segment) {
        frames_by_segment.push((key, group.collect()));
    }

    for (segment, frames) in frames_by_segment {
        let path = get_path_for_segment(segment, src_dir);
        let file = File::open(path).unwrap();
        let mut reader = BufReader::new(file);

        for f in frames {
            let mut f_reader = MlpFrameReader::new(f, &mut reader);
            std::io::copy(&mut f_reader, writer).map(|_| ())?;
        }
    }

    Ok(())
}

fn get_path_for_segment(segment: u16, src_dir: &Path) -> PathBuf {
    let mut buf = PathBuf::from(src_dir);
    let filename = format!("{:0>5}.m2ts", segment);
    buf.push(filename);
    buf
}
