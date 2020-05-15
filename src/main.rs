use clap::{App, Arg, ArgGroup};
use itertools::Itertools;
use libav::DemuxOptions;
use mlp::{MlpFrame, MlpFrameReader, MlpIterator};
use num_format::{Locale, ToFormattedString};
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
                    Arg::with_name("audio-threshold")
                        .long("threshold")
                        .value_name("THRESHOLD")
                        .default_value("1024")
                        .validator(|s| {
                            s.parse::<i32>().map_err(|_| String::from("Must be a number."))
                        })
                        .required(false)
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
contain different audio."
                        ),
                ),
        )
        .subcommand(
            App::new("info")
                .about("Prints information about the TrueHD stream.")
                .arg("-s, --stream <FILE> 'The TrueHD stream.'"),
        )
        .subcommand(
            App::new("ff")
                .arg("-i, --input <FILE>")
                .arg("-h, --head 'print first frame'")
                .arg("-t, --tail 'print last frame'"),
        )
        .get_matches();

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
                    dbg!(&segment_paths);

                    let mut writer = BufWriter::new(out_file);
                    let demux_opts = DemuxOptions {
                        audio_match_threshold: threshold,
                    };
                    let overrun =
                        libav::demux_thd(&segment_paths, &demux_opts, &mut writer).unwrap();
                    dbg!(overrun.samples());
                } else {
                    println!("segment list invalid.");
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
                println!("head: {}", head);
                libav::thd_audio_read_test(&input_path, head, threshold);
            } else {
                println!("wrong args");
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

    let mut num_frames = 0;
    let mut num_major_frames = 0;
    for frame in MlpIterator::new(BufReader::new(file)) {
        if frame.has_major_sync {
            num_major_frames += 1;
        }
        num_frames += 1;
    }

    let duration = (num_frames * 40) as f64 / 48000_f64;

    println!("Assuming 48 KHz sampling frequency and 40 samples per frame.");
    println!(
        "Total MLP frame count: {:>14}",
        num_frames.to_formatted_string(&Locale::en)
    );
    println!(
        "  Major frames: {:>21}",
        num_major_frames.to_formatted_string(&Locale::en)
    );
    println!(
        "  Minor frames: {:>21}",
        (num_frames - num_major_frames).to_formatted_string(&Locale::en)
    );
    println!(
        "Number of audio samples: {:>12}",
        (num_frames * 40).to_formatted_string(&Locale::en)
    );
    println!("Duration: {:>35.7} seconds", duration);

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
