use ::log::*;
use ffmpeg4_ffi::sys as ff;
use std::path::PathBuf;

pub mod av_codec_context;
pub use av_codec_context::AVCodecContext;

pub mod av_error;
pub use av_error::{AVError, DemuxErr, OtherErr};

pub mod av_format_context;
pub use av_format_context::{AVCodecType, AVFormatContext, AVStream};

pub mod av_frame;
pub use av_frame::AVFrame;

pub mod av_resample;
pub use av_resample::{SwrContext, SwrOptions};

pub mod av_log;

pub mod av_packet;
pub use av_packet::AVPacket;

pub mod truehd;
pub use truehd::{
    DecodedThdFrame, ThdDecodePacket, ThdFrameHeader, ThdOverrun, ThdSample, ThdSegment,
};

pub mod demux;
pub use demux::DemuxStats;

pub mod dsp;

impl<'a> From<&AVFrame<'a>> for DecodedThdFrame {
    fn from(frame: &AVFrame<'a>) -> Self {
        let bytes = frame.as_slice();
        let bytes_per_sample = frame.bytes_per_sample();
        let sample_rate = frame.sample_rate();
        let channels = frame.channels();
        DecodedThdFrame::new(bytes, bytes_per_sample, channels, sample_rate)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Framerate {
    pub numerator: i32,
    pub denominator: i32,
}

#[derive(Debug, Copy, Clone)]
pub struct VideoMetadata {
    pub framerate: Framerate,
}

pub trait MediaDuration {
    fn duration(&self, frames: u32) -> f64;
}

impl MediaDuration for VideoMetadata {
    fn duration(&self, frames: u32) -> f64 {
        (frames as i32 * self.framerate.denominator) as f64 / self.framerate.numerator as f64
    }
}

fn print_frames(frames: &Vec<DecodedThdFrame>) {
    for sample in frames.iter().flat_map(|f| f.samples.iter()) {
        println!("ch[{}], s[{}]", sample.channel, sample.value)
    }
}

// used for development and testing
pub fn thd_audio_read_test(file: &PathBuf, head: bool) -> Result<(), AVError> {
    let file_path = file
        .to_str()
        .ok_or(OtherErr::FilePathIsNotUtf8(PathBuf::from(file)))?;
    let mut avctx = AVFormatContext::open(&file_path)?;
    let streams = &avctx.streams()?;

    info!("processing file '{}' ...", file_path);

    let audio_stream = streams
        .iter()
        .find(|&s| s.codec.id == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
        .ok_or(DemuxErr::NoTrueHdStreamFound)?;

    if head {
        match demux::decode_head_frame(&mut avctx, &audio_stream)? {
            Some(frame) => {
                // println!("is silence: {}", &frame.original.is_silence(threshold));
                print_frames(&vec![frame.original]);
            }
            None => {
                println!("no frames encountered");
            }
        }
    } else {
        match demux::decode_tail_frame(&mut avctx, &audio_stream)? {
            Some(frame) => {
                // println!("is silence: {}", &frame.original.is_silence(threshold));
                print_frames(&vec![frame.original]);
            }
            None => {
                println!("no frames encountered");
            }
        }
    }

    Ok(())
}
