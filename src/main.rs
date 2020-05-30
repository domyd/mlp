use anyhow::Context;
use clap::{crate_version, App, Arg, ArgGroup, ArgSettings};
use libav::{demux::ThdStreamInfo, truehd::ThdMetadata, MediaDuration};
use log::*;
use mpls::{Mpls, PlayItem};
use num_format::{Locale, ToFormattedString};
use simplelog::*;
use std::fs::File;
use std::{
    io::BufWriter,
    path::{Path, PathBuf},
};

pub mod libav;

fn main() -> anyhow::Result<()> {
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
                            Arg::with_name("stream-idx")
                            .about("Sets the index of the TrueHD stream to demux.")
                            .long("stream")
                            .required(false)
                            .takes_value(true)
                            .validator(|s| {
                                s.parse::<i32>()
                                    .map_err(|_| String::from("Must be a number."))
                            }),
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
                            Arg::with_name("stream-idx")
                            .about("Sets the index of the TrueHD stream to demux.")
                            .long("stream")
                            .required(false)
                            .takes_value(true)
                            .validator(|s| {
                                s.parse::<i32>()
                                    .map_err(|_| String::from("Must be a number."))
                            }),
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
                ),
        )
        .subcommand(
            App::new("info")
                .about("Prints information about the TrueHD stream.")
                .arg(Arg::with_name("stream").value_name("STREAM").required(true))
                .arg(
                    Arg::with_name("stream-idx")
                    .about("Sets the index of the TrueHD stream to demux.")
                    .long("stream")
                    .required(false)
                    .takes_value(true)
                    .validator(|s| {
                        s.parse::<i32>()
                            .map_err(|_| String::from("Must be a number."))
                    }),
                ),
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
            match sub.subcommand() {
                ("playlist", Some(sub)) => {
                    let mpls_path = sub.value_of("playlist").map(|p| PathBuf::from(p)).unwrap();

                    // turn angle into a 0-based index internally
                    let user_did_supply_angle = sub.occurrences_of("angle") > 0;
                    let angle_arg = sub
                        .value_of("angle")
                        .map(|s| s.parse::<i32>().unwrap())
                        .map(|n| n.max(1) - 1)
                        .unwrap();

                    let user_stream_idx = sub
                        .value_of("stream-idx")
                        .map(|s| s.parse::<i32>().unwrap());

                    let mpls = {
                        let file = File::open(&mpls_path)?;
                        let mpls = Mpls::from(file).expect("failed to parse MPLS file.");
                        mpls
                    };
                    let segments = {
                        let angles = mpls.angles();
                        if angles.len() > 1 && !user_did_supply_angle {
                            warn!("This playlist contains more than one angle, but you did not select an angle with --angle. Using the default angle 1 ...");
                        }
                        let selected_angle = match angles.get(angle_arg as usize) {
                            None => {
                                error!("Angle {} doesn't exist in this playlist.", angle_arg + 1);
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

                        get_segments(&mpls, &selected_angle, &mpls_path)
                    };

                    let thd_streams = libav::demux::thd_streams(&segments[0].path)
                        .map(|s| thd_streams_with_language(&s, &mpls.play_list.play_items[0]))
                        .context("Failed at searching for TrueHD streams.")?;
                    print_thd_stream_list(&thd_streams);

                    if let Some(output_path) = sub.value_of("output").map(|p| PathBuf::from(p)) {
                        let selected_stream = select_thd_stream(&thd_streams, user_stream_idx)?;
                        let demux_opts = match selected_stream {
                            Some(i) => libav::demux::DemuxOptions {
                                thd_stream_id: Some(i),
                            },
                            None => {
                                return Ok(());
                            }
                        };

                        if let Some(file) =
                            file_create_with_force_check(&output_path, force).transpose()?
                        {
                            let writer = BufWriter::new(file);
                            let stats = libav::demux::demux_thd(&segments, &demux_opts, writer)
                                .context("Failed demuxing TrueHD stream.")?;
                            print_demux_stats(&stats);
                        }
                    } else {
                        let mpls = {
                            let f = File::open(&mpls_path).with_context(|| {
                                format!("Failed to open MPLS file at {}", &mpls_path.display())
                            })?;
                            Mpls::from(f)?
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

                    let user_stream_idx = sub
                        .value_of("stream-idx")
                        .map(|s| s.parse::<i32>().unwrap());

                    let segments: Vec<Segment> = {
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
                                .map(|s| Segment {
                                    path: s,
                                    video_frames: None,
                                })
                                .collect()
                        } else if let Some(values) = sub.values_of("segment-files") {
                            values
                                .map(|s| {
                                    let mut p = source_dir_path.clone();
                                    p.push(s);
                                    p
                                })
                                .map(|s| Segment {
                                    path: s,
                                    video_frames: None,
                                })
                                .collect()
                        } else {
                            // can't happen, clap makes sure of that
                            return Ok(());
                        }
                    };

                    let thd_streams = libav::demux::thd_streams(&segments[0].path)
                        .context("Failed at searching for TrueHD streams.")?;
                    print_thd_stream_list(&thd_streams);
                    let selected_stream = select_thd_stream(&thd_streams, user_stream_idx)?;
                    let demux_opts = match selected_stream {
                        Some(i) => libav::demux::DemuxOptions {
                            thd_stream_id: Some(i),
                        },
                        None => {
                            return Ok(());
                        }
                    };

                    if let Some(file) =
                        file_create_with_force_check(&output_path, force).transpose()?
                    {
                        let writer = BufWriter::new(file);
                        let stats = libav::demux::demux_thd(&segments, &demux_opts, writer)
                            .context("Failed demuxing TrueHD stream.")?;
                        print_demux_stats(&stats);
                    }

                    Ok(())
                }
                _ => Ok(()),
            }
        }
        ("info", Some(sub)) => {
            let path = sub.value_of("stream").map(|p| PathBuf::from(p)).unwrap();
            let user_stream_idx = sub
                .value_of("stream-idx")
                .map(|s| s.parse::<i32>().unwrap());

            if let Some((a, b, metadata)) = count_thd_frames(&path, user_stream_idx)? {
                print_frame_count_info((a, b), &metadata);
            }

            Ok(())
        }
        _ => Ok(()),
    }
}

fn print_thd_stream_list(streams: &[ThdStreamInfo]) {
    for s in streams {
        info!("{}", s);
    }
}

fn thd_streams_with_language(streams: &[ThdStreamInfo], mpls: &PlayItem) -> Vec<ThdStreamInfo> {
    let langs = mpls
        .stream_number_table
        .primary_audio_streams
        .iter()
        .filter_map(|s| {
            if let Some(lang) = match s.attrs.stream_type {
                mpls::StreamType::Audio(_, _, ref lang) => Some(lang.clone()),
                _ => None,
            } {
                if let Some(pid) = match s.entry.refs {
                    mpls::StreamEntryRef::PlayItem(r) => match r {
                        mpls::Ref::Stream(i) => Some(i.0 as i32),
                        _ => None,
                    },
                    _ => None,
                } {
                    return Some((pid, lang));
                }
            }

            return None;
        })
        .collect::<Vec<(i32, String)>>();

    streams
        .into_iter()
        .map(|stream| {
            if let Some(lang) = langs.iter().find_map(|s| {
                if s.0 == stream.id {
                    Some(s.1.clone())
                } else {
                    None
                }
            }) {
                ThdStreamInfo {
                    language: Some(lang),
                    ..stream.clone()
                }
            } else {
                stream.clone()
            }
        })
        .collect()
}

fn select_thd_stream(
    streams: &[libav::demux::ThdStreamInfo],
    user_select: Option<i32>,
) -> Result<Option<i32>, libav::AVError> {
    if streams.len() == 0 {
        Err(libav::AVError::DemuxErr(
            libav::DemuxErr::NoTrueHdStreamFound,
        ))
    } else if streams.len() == 1 {
        let s = streams.first().unwrap();
        if let Some(i) = user_select {
            if i != s.index {
                warn!(
                    "Only one TrueHD stream found with index {}, using that one instead.",
                    s.index
                );
            }
        }
        Ok(Some(s.id))
    } else {
        if let Some(i) = user_select {
            if let Some(s) = streams.iter().find(|s| s.index == i) {
                Ok(Some(s.id))
            } else {
                Err(libav::AVError::DemuxErr(
                    libav::DemuxErr::SelectedTrueHdStreamNotFound(i),
                ))
            }
        } else {
            warn!(
                "Found multiple ({}) TrueHD streams. Re-run with --stream <INDEX> to select one.",
                streams.len()
            );
            Ok(None)
        }
    }
}

fn file_create_with_force_check<P: AsRef<Path>>(
    path: P,
    force: bool,
) -> Option<anyhow::Result<File>> {
    match (path.as_ref().exists(), force) {
        (true, true) => {
            warn!(
                "Overwriting existing output file {}.",
                path.as_ref().display()
            );
            Some(File::create(&path).with_context(|| {
                format!(
                    "Failed to create output file at {}",
                    path.as_ref().display()
                )
            }))
        }
        (true, false) => {
            warn!(
                "Output file {} already exists. Use the --force to overwrite existing output files.",
                path.as_ref().display()
            );
            None
        }
        (false, _) => Some(File::create(&path).with_context(|| {
            format!(
                "Failed to create output file at {}",
                path.as_ref().display()
            )
        })),
    }
}

fn count_thd_frames<P: AsRef<Path>>(
    filepath: P,
    stream_idx: Option<i32>,
) -> anyhow::Result<Option<(i32, i32, ThdMetadata)>> {
    let thd_streams = libav::demux::thd_streams(&filepath)?;
    print_thd_stream_list(&thd_streams);
    if let Some(stream_pid) = select_thd_stream(&thd_streams, stream_idx)? {
        info!("Counting output file frames ...");

        let mut avctx = libav::AVFormatContext::open(&filepath)?;
        let streams = avctx.streams()?;
        let thd_stream = streams
            .iter()
            .find(|&s| {
                s.codec.id == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD
                    && s.stream.id == stream_pid
            })
            .ok_or(libav::AVError::DemuxErr(
                libav::DemuxErr::NoTrueHdStreamFound,
            ))?;

        let metadata = libav::demux::get_thd_metadata(&thd_stream);

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

        Ok(Some((num_frames, num_major_frames, metadata)))
    } else {
        Ok(None)
    }
}

pub struct Segment {
    pub path: PathBuf,
    pub video_frames: Option<i32>,
}

fn get_segments(playlist: &Mpls, angle: &mpls::Angle, playlist_path: &PathBuf) -> Vec<Segment> {
    // find the blu-ray STREAM directory, relative to the
    // playlist path
    let stream_dir = {
        let mut p = playlist_path.clone();
        p.pop();
        p.pop();
        p.push("STREAM");
        p
    };

    let mut segments = Vec::new();
    for play_item in &playlist.play_list.play_items {
        let clip = play_item.clip_for_angle(angle);
        let clip_path = {
            let mut path = stream_dir.clone();
            path.push(&clip.file_name);
            path.set_extension("m2ts");
            path
        };
        let video_frames = {
            let len = play_item.out_time.seconds() - play_item.in_time.seconds();
            let v_stream = play_item
                .stream_number_table
                .primary_video_streams
                .first()
                .unwrap();
            let fps = match v_stream.attrs.stream_type {
                mpls::StreamType::HdrVideo(_, f, _, _) => f.unwrap().fps(),
                mpls::StreamType::SdrVideo(_, f) => f.unwrap().fps(),
                _ => return Vec::new(),
            };
            Some((len * fps).round() as i32)
        };

        segments.push(Segment {
            path: clip_path,
            video_frames,
        });
    }

    segments
}

fn print_demux_stats(stats: &libav::DemuxStats) {
    let (video_frames, audio_frames) = stats
        .segments
        .iter()
        .map(|s| (s.video_frames, s.thd_frames))
        .fold((0, 0), |(va, aa), (v, a)| (va + v, aa + a));
    let (video_meta, audio_meta) = (
        stats.video_metadata().unwrap(),
        stats.thd_metadata().unwrap(),
    );
    let (video_duration, audio_duration) = (
        video_meta.duration(video_frames),
        audio_meta.duration(audio_frames),
    );

    let target_audio_len = video_duration + stats.segments.last().unwrap().audio_overrun();
    let samples_off_target = ((audio_duration - target_audio_len) * 48000f64).round() as i32;

    info!(
        "Video length: {:>16} frames ({:.7} seconds)",
        video_frames.to_formatted_string(&Locale::en),
        video_duration
    );
    info!(
        "Audio length: {:>16} frames ({:.7} seconds)",
        audio_frames.to_formatted_string(&Locale::en),
        audio_duration
    );
    let print_alignment = 26 + target_audio_len.trunc().to_string().len();
    info!(
        "Target audio length: {:>a$.7} seconds",
        target_audio_len,
        a = print_alignment
    );
    info!(
        "Audio samples off target: {:>4.0} {}",
        samples_off_target,
        match samples_off_target {
            -40..=40 => "(🟢 perfect)",
            -80..=80 => "(🟡 suboptimal)",
            _ => "(🔴 please file issue at https://github.com/domyd/mlp/issues)",
        }
    );
}

fn print_frame_count_info(counter: (i32, i32), metadata: &ThdMetadata) {
    let (num_frames, num_major_frames) = counter;

    let duration = (num_frames * 40) as f64 / 48000_f64;

    info!(
        "Assuming {} Hz sampling frequency and {} samples per frame.",
        metadata.sample_rate, metadata.frame_size
    );
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
