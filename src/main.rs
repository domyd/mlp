use std::fs::File;
use std::{path::{PathBuf, Path}, io::{BufReader, Write, BufWriter}};
use clap::{Arg, App};
use itertools::Itertools;
use mlp::{MlpIterator, MlpFrame, MlpFrameReader};
pub mod mlp;

fn main() -> std::io::Result<()> {
    let args = App::new("MLP Combiner")
        .version("0.1")
        .author("Dominik Mydlil <dominik.mydlil@outlook.com>")
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
        .arg("<OUTPUT>                 'Sets the output file.'")
        .get_matches();

    let out_file_str: &str = args.value_of("OUTPUT").unwrap();
    let out_file = File::create(out_file_str).expect(format!("Failed to create file '{0}'.", out_file_str).as_ref());

    let src_dir_str: &str = args.value_of("source").unwrap();
    let src_dir_buf = PathBuf::from(src_dir_str);

    let map_values: Result<Vec<u16>, _> = args.values_of("map").unwrap()
        .map(|s| s.parse::<u16>())
        .collect();
    let segments = map_values.expect("some segments in the map aren't numbers.");

    let frames: Vec<MlpFrame> = segments.iter()
        .flat_map(|s| {
            let path = get_path_for_segment(*s, src_dir_buf.as_path());
            let file = File::open(path).unwrap();
            MlpIterator::new(BufReader::new(file), *s)
        })
        .collect();

    let mut out_file_writer = BufWriter::new(out_file);
    write_mlp_frames(&frames, src_dir_buf.as_path(), &mut out_file_writer)?;

    Ok(())
}

fn write_mlp_frames<W: Write>(frames: &[MlpFrame], src_dir: &Path, writer: &mut W) -> std::io::Result<()> {
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
    let filename = format!("{0}.thd", segment);
    buf.push(filename);
    buf
}
