use clap::{App, Arg};
use itertools::Itertools;
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
    let args = App::new("MLP Combiner")
        .version("0.1")
        .author("Dominik Mydlil <dominik.mydlil@outlook.com>")
        .about("Dolby TrueHD utility tool")
        .subcommand(App::new("append")
            .about("Appends individual TrueHD streams together")
            .arg("-s, --source <DIRECTORY> 'Sets the directory that contains the m2ts source files.'")
            .arg(Arg::with_name("map")
                .short('m')
                .long("map")
                .takes_value(true)
                .required(true)
                .min_values(2)
                .value_delimiter(",")
                .about("Sets the ordered list of TrueHD segments."))
            .arg("<OUTPUT>                 'Sets the output file.'"))
        .subcommand(App::new("info")
            .about("Prints information about the TrueHD stream.")
            .arg("-s, --stream <FILE> 'The TrueHD stream.'"))
        .subcommand(App::new("ff")
            .arg("-i, --input <FILE>")
            .arg("-h, --head 'print first frame'")
            .arg("-t, --tail 'print last frame'"))
        .get_matches();

    match args.subcommand() {
        ("append", Some(sub)) => {
            let out_file_str: &str = sub.value_of("OUTPUT").unwrap();
            let out_file = File::create(out_file_str)
                .expect(format!("Failed to create file '{0}'.", out_file_str).as_ref());

            let src_dir_str: &str = sub.value_of("source").unwrap();
            let src_dir_buf = PathBuf::from(src_dir_str);

            let map_values: Result<Vec<PathBuf>, _> = sub
                .values_of("map")
                .unwrap()
                .map(|s| {
                    s.parse::<u16>()
                        .map(|n| get_path_for_segment(n, src_dir_buf.as_path()))
                })
                .collect();
            let segments = map_values.expect("some segments in the map aren't numbers.");

            dbg!(&segments);

            let mut writer = BufWriter::new(out_file);
            let overrun =
                libav::concat_thd_from_m2ts(&segments, &mut writer).unwrap();
            dbg!(overrun.samples());

            // let frames: Vec<MlpFrame> = segments
            //     .iter()
            //     .flat_map(|s| {
            //         let path = get_path_for_segment(*s, src_dir_buf.as_path());
            //         let file = File::open(path).unwrap();
            //         MlpIterator::with_segment(BufReader::new(file), *s)
            //     })
            //     .collect();

            //let mut out_file_writer = BufWriter::new(out_file);
            //write_mlp_frames(&frames, src_dir_buf.as_path(), &mut out_file_writer)?;

            Ok(())
        }
        ("info", Some(sub)) => {
            let path = PathBuf::from(sub.value_of("stream").unwrap());
            print_stream_info(path.as_path())?;
            Ok(())
        }
        ("ff", Some(sub)) => {
            let input_path = PathBuf::from(sub.value_of("input").unwrap());
            if let Some(head) = match sub.is_present("head") {
                true => Some(true),
                false => match sub.is_present("tail") {
                    true => Some(false),
                    false => None,
                },
            } {
                println!("head: {}", head);
                libav::thd_audio_read_test(&input_path, head);
            } else {
                println!("wrong args");
            }

            Ok(())
        }
        _ => Ok(()),
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
