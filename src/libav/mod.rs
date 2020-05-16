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

pub mod av_log;

pub mod av_packet;
pub use av_packet::{AVPacket, AVPacketReader};

pub mod truehd;
pub use truehd::{DecodedThdFrame, ThdFrameHeader, ThdOverrun, ThdSample, ThdSegment};

pub mod demux;
pub use demux::DemuxOptions;

impl From<&AVFrame> for DecodedThdFrame {
    fn from(frame: &AVFrame) -> Self {
        let bytes = frame.as_slice();
        let bytes_per_sample = frame.bytes_per_sample();
        let channels = frame.channels();
        DecodedThdFrame::new(bytes, bytes_per_sample, channels)
    }
}

fn print_frames(frames: &Vec<DecodedThdFrame>) {
    for sample in frames.iter().flat_map(|f| f.samples.iter()) {
        println!("ch[{}], s[{}]", sample.channel, sample.value)
    }
}

// used for development and testing
pub fn thd_audio_read_test(file: &PathBuf, head: bool, threshold: i32) -> Result<(), AVError> {
    let file_path = file
        .to_str()
        .ok_or(OtherErr::FilePathIsNotUtf8(PathBuf::from(file)))?;
    let avctx = AVFormatContext::new(&file_path)?;
    let streams = avctx.get_streams()?;

    info!("processing file '{}' ...", file_path);

    let audio_stream = streams
        .iter()
        .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
        .ok_or(DemuxErr::NoTrueHdStreamFound)?;

    if head {
        match demux::decode_head_frame(&avctx, &audio_stream)? {
            Some(frame) => {
                println!("is silence: {}", &frame.is_silence(threshold));
                print_frames(&vec![frame]);
            }
            None => {
                println!("no frames encountered");
            }
        }
    } else {
        match demux::decode_tail_frame(&avctx, &audio_stream)? {
            Some(frame) => {
                println!("is silence: {}", &frame.is_silence(threshold));
                print_frames(&vec![frame]);
            }
            None => {
                println!("no frames encountered");
            }
        }
    }

    Ok(())
}
