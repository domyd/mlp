use clap::{crate_version, App, Arg, ArgGroup, ArgSettings};
use log::*;
use mpls::Mpls;
use num_format::{Locale, ToFormattedString};
use simplelog::*;
use std::fs::File;
use std::{
    io::{BufWriter},
    path::{Path, PathBuf},
};
use libav::DemuxOptions;

pub mod libav;

fn main() -> std::io::Result<()> {
    let args = App::new("TrueHD Demuxer")
        .version(crate_version!())
        .author("Dominik Mydlil <dominik.mydlil@outlook.com>")
        .about("A Dolby TrueHD demuxer and utility tool")
        .subcommand(
            App::new("demux")
                .about("Demux TrueHD stream from blu-ray files.")
                .subcommand(
                    App::new("playlist")
                        .about("Demux from a blu-ray playlist file.")
                        .arg(
                            Arg::with_name("playlist")
                                .about("Sets the path to the playlist file (.mpls).")
                                .value_name("PLAYLIST")
                                .required(true),
                        )
                        .arg(
                            Arg::with_name("output")
                                .long_about("Sets the output TrueHD file. If omitted, playlist info will be printed instead.")
                                .short('o')
                                .long("output")
                                .value_name("OUTPUT")
                                .takes_value(true)
                                .required(false),
                        )
                        .arg(
                            Arg::with_name("angle")
                                .about("Sets the playlist angle index, starting at 1.")
                                .long("angle")
                                .default_value("1")
                                .value_name("ANGLE")
                                .required(false)
                                .validator(|s| {
                                    s.parse::<i32>()
                                        .map_err(|_| String::from("Must be a number."))
                                }),
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
                            Arg::with_name("segment-files")
                                .about("Sets the list of file names, separated by +.")
                                .long("segment-files")
                                .value_delimiter("+")
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
                            s.parse::<i32>()
                                .map_err(|_| String::from("Must be a number."))
                        })
                        .global(true)
                        .about("Determines the sensitivity of the TrueHD frame comparer.")
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
",
                        ),
                ),
        )
        .subcommand(
            App::new("info")
                .about("Prints information about the TrueHD stream.")
                .arg(Arg::with_name("stream").value_name("STREAM").required(true)),
        )
        .arg(
            Arg::with_name("verbosity")
                .about("Sets the output verbosity.")
                .global(true)
                .setting(ArgSettings::MultipleOccurrences)
                .short('v'),
        )
        .arg(
            Arg::with_name("force")
                .about("Overwrite existing output files.")
                .global(true)
                .long("force")
                .short('f'),
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
",
                )
                .global(true)
                .long("enable-ffmpeg-log"),
        )
        .after_help("This software uses libraries from the FFmpeg project under the LGPLv2.1.")
        .get_matches();

    let force = args.is_present("force");
    let verbosity_level = args.occurrences_of("verbosity").min(3);
    let log_ffmpeg = args.is_present("ffmpeg-log");

    setup_logging(verbosity_level as i32, log_ffmpeg);

    match args.subcommand() {
        ("demux", Some(sub)) => {
            let threshold = sub
                .value_of("threshold")
                .map(|s| s.parse::<i32>().unwrap())
                .unwrap();

            match sub.subcommand() {
                ("playlist", Some(sub)) => {
                    let mpls_path = sub
                        .value_of("playlist")
                        .map(|p| PathBuf::from(p))
                        .expect("invalid MPLS path");
                    // find the blu-ray STREAM directory, relative to the
                    // playlist path
                    let stream_dir = {
                        let mut p = mpls_path.clone();
                        p.pop();
                        p.pop();
                        p.push("STREAM");
                        p
                    };
                    // turn angle into a 0-based index internally
                    let user_did_supply_angle = sub.occurrences_of("angle") > 0;
                    let angle_arg = sub
                        .value_of("angle")
                        .map(|s| s.parse::<i32>().unwrap())
                        .map(|n| n.max(1) - 1)
                        .unwrap();

                    if let Some(output_path) = sub.value_of("output").map(|p| PathBuf::from(p)) {
                        let segment_paths = {
                            let file = File::open(&mpls_path)?;
                            let mpls = Mpls::from(file).expect("failed to parse MPLS file.");
                            let angles = mpls.angles();
                            if angles.len() > 1 && !user_did_supply_angle {
                                warn!("This playlist contains more than one angle, but you did not select an angle with --angle. Using the default angle 1 ...");
                            }
                            let selected_angle = match angles.get(angle_arg as usize) {
                                None => {
                                    error!(
                                        "Angle {} doesn't exist in this playlist.",
                                        angle_arg + 1
                                    );
                                    return Ok(());
                                }
                                Some(a) => a,
                            };

                            debug!(
                                "Playlist has {} {}.",
                                angles.len(),
                                if angles.len() > 1 { "angles" } else { "angle" }
                            );
                            debug!("Using angle {}.", angle_arg + 1);

                            let segment_paths: Vec<PathBuf> = selected_angle
                                .segments()
                                .iter()
                                .map(|s| {
                                    let mut path = stream_dir.clone();
                                    path.push(&s.file_name);
                                    path.set_extension("m2ts");
                                    path
                                })
                                .collect();
                            segment_paths
                        };

                        if let Some(file) =
                            file_create_with_force_check(&output_path, force).transpose()?
                        {
                            let writer = BufWriter::new(file);
                            let demux_opts = DemuxOptions {
                                audio_match_threshold: threshold,
                            };
                            let _overrun =
                                libav::demux::demux_thd(&segment_paths, &demux_opts, writer)
                                    .unwrap();

                            let fc = count_thd_frames(&output_path)
                                .expect("failed to count TrueHD frames");
                            print_frame_count_info(fc);
                        }
                    } else {
                        let mpls = {
                            let f = File::open(&mpls_path)?;
                            Mpls::from(f).expect("failed to parse MPLS file.")
                        };
                        print_playlist_info(&mpls);
                    }

                    Ok(())
                }
                ("segments", Some(sub)) => {
                    let output_path = sub.value_of("output").map(|p| PathBuf::from(p)).unwrap();
                    let source_dir_path = sub
                        .value_of("stream-dir")
                        .map(|p| PathBuf::from(p))
                        .unwrap();

                    let segments: Vec<PathBuf> = {
                        if let Some(values) = sub.values_of("segment-list") {
                            values
                                .map(|s| {
                                    s.parse::<u16>()
                                        .expect("segment list must only contain numbers.")
                                })
                                .map(|s| {
                                    let mut p = source_dir_path.clone();
                                    p.push(format!("{:0>5}.m2ts", s));
                                    p
                                })
                                .collect()
                        } else if let Some(values) = sub.values_of("segment-files") {
                            values
                                .map(|s| {
                                    let mut p = source_dir_path.clone();
                                    p.push(s);
                                    p
                                })
                                .collect()
                        } else {
                            // can't happen, clap makes sure of that
                            return Ok(());
                        }
                    };

                    if let Some(file) =
                        file_create_with_force_check(&output_path, force).transpose()?
                    {
                        let mut writer = BufWriter::new(file);
                        let demux_opts = DemuxOptions {
                            audio_match_threshold: threshold,
                        };
                        let _overrun =
                            libav::demux::demux_thd(&segments, &demux_opts, &mut writer).unwrap();

                        let fc =
                            count_thd_frames(&output_path).expect("failed to count TrueHD frames");
                        print_frame_count_info(fc);
                    }

                    Ok(())
                }
                _ => Ok(()),
            }
        }
        ("info", Some(sub)) => {
            let path = sub.value_of("stream").map(|p| PathBuf::from(p)).unwrap();
            let frame_count = count_thd_frames(&path).unwrap();
            print_frame_count_info(frame_count);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn file_create_with_force_check<P: AsRef<Path>>(
    path: P,
    force: bool,
) -> Option<Result<File, std::io::Error>> {
    match (path.as_ref().exists(), force) {
        (true, true) => {
            warn!(
                "Overwriting existing output file {}.",
                path.as_ref().display()
            );
            Some(File::create(&path))
        }
        (true, false) => {
            warn!(
                "Output file {} already exists. Use the --force to overwrite existing output files.",
                path.as_ref().display()
            );
            None
        }
        (false, _) => Some(File::create(&path)),
    }
}

fn count_thd_frames<P: AsRef<Path>>(filepath: P) -> Result<(i32, i32), libav::AVError> {
    info!("Counting output file frames ...");

    let avctx = libav::AVFormatContext::open(&filepath)?;
    let streams = avctx.get_streams()?;
    let thd_stream = streams
        .iter()
        .find(|&s| s.codec_id() == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD)
        .ok_or(libav::DemuxErr::NoTrueHdStreamFound)?;

    let (mut num_frames, mut num_major_frames) = (0i32, 0i32);
    while let Ok(packet) = avctx.read_frame() {
        if packet.of_stream(&thd_stream) {
            let frame = libav::ThdFrameHeader::from_bytes(&packet.as_slice()).unwrap();
            if frame.has_major_sync {
                num_major_frames += 1;
            }
            num_frames += 1;
        }
    }

    Ok((num_frames, num_major_frames))
}

fn print_frame_count_info(counter: (i32, i32)) {
    let (num_frames, num_major_frames) = counter;

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
}

fn print_playlist_info(playlist: &Mpls) {
    let n_segments = playlist.play_list.play_items.len();
    let angles = playlist.angles();
    let n_angles = angles.len();
    info!(
        "Playlist contains {} segments and {} angle{}.",
        n_segments,
        n_angles,
        if n_angles > 1 { "s" } else { "" }
    );
    for angle in angles {
        let segment_numbers: Vec<i32> = angle
            .segments()
            .iter()
            .map(|s| s.file_name.parse::<i32>().unwrap())
            .collect();
        if n_angles > 1 {
            info!(
                "Segments for angle {}: {:?}",
                angle.index + 1,
                segment_numbers
            );
        } else {
            info!("Segments: {:?}", segment_numbers);
        }
    }
}

fn setup_logging(verbosity_level: i32, log_ffmpeg: bool) {
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
        if !log_ffmpeg {
            builder.add_filter_ignore_str("ffmpeg");
        }
        builder.build()
    };
    TermLogger::init(verbosity, logger_config, TerminalMode::Mixed).unwrap();
    libav::av_log::configure_rust_log(ffmpeg_log_level);
}
