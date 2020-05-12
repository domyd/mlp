use ffmpeg4_ffi::sys as ff;
use ffmpeg4_ffi::sys::{
    AVMediaType_AVMEDIA_TYPE_AUDIO as AVMEDIA_TYPE_AUDIO,
    AVMediaType_AVMEDIA_TYPE_VIDEO as AVMEDIA_TYPE_VIDEO,
};
use std::io::Read;

pub mod av_codec_context;
pub mod av_format_context;
pub mod av_packet;
pub use av_codec_context::AVCodecContext;
pub use av_format_context::AVFormatContext;
pub use av_packet::{AVPacket, AVPacketReader};

#[derive(Debug, Clone)]
pub struct AVError {
    pub err: i32,
}

pub struct AVStream {
    idx: i32,
    codec: *mut ff::AVCodec,
    codec_params: *mut ff::AVCodecParameters,
}

impl AVStream {
    pub fn get_codec_context(&self) -> Result<AVCodecContext, AVError> {
        let ctx = AVCodecContext::new(self)?;
        Ok(ctx)
    }
}

fn is_true_hd_stream(stream: &AVStream) -> bool {
    unsafe {
        (*stream.codec).id == ff::AVCodecID_AV_CODEC_ID_TRUEHD
            && (*stream.codec_params).codec_type == ff::AVMediaType_AVMEDIA_TYPE_AUDIO
    }
}

pub fn read_thd_stream(path: &str) -> Vec<u8> {
    let avctx = AVFormatContext::new(path).unwrap();
    let streams = avctx.get_streams().unwrap();

    let video_stream = streams
        .iter()
        .find(|&s| unsafe { (*s.codec_params).codec_type == AVMEDIA_TYPE_VIDEO })
        .unwrap();
    let audio_stream = streams.iter().find(|&s| is_true_hd_stream(s)).unwrap();

    let v_ctx = video_stream.get_codec_context().unwrap();
    v_ctx.open(&video_stream).unwrap();
    let a_ctx = audio_stream.get_codec_context().unwrap();
    a_ctx.open(&audio_stream).unwrap();

    let mut packet = AVPacket::new().unwrap();

    let mut bytes_read = 0usize;
    while let Some(()) = avctx.read_frame(&mut packet) {
        if unsafe { (*packet.pkt).stream_index } == audio_stream.idx {
            let mut v: Vec<u8> = Vec::with_capacity(unsafe {(*packet.pkt).size} as usize);
            bytes_read += AVPacketReader::new(&packet).read_to_end(&mut v).unwrap();
            dbg!(bytes_read);
        }
    }

    let r: Vec<u8> = Vec::new();
    return r;
}

pub fn count_video_frames(path: &str) -> Option<u32> {
    let avctx = AVFormatContext::new(path).unwrap();
    let streams = avctx.get_streams().unwrap();

    let video_stream = streams
        .iter()
        .find(|&s| unsafe { (*s.codec_params).codec_type == AVMEDIA_TYPE_VIDEO })
        .unwrap();
    let audio_stream = streams.iter().find(|&s| is_true_hd_stream(s)).unwrap();

    let v_ctx = video_stream.get_codec_context().unwrap();
    v_ctx.open(&video_stream).unwrap();
    let a_ctx = audio_stream.get_codec_context().unwrap();
    a_ctx.open(&audio_stream).unwrap();

    let mut packet = AVPacket::new().unwrap();

    let mut i: u32 = 0;
    let mut thd_vec: Vec<u8> = Vec::new();
    while let Some(()) = avctx.read_frame(&mut packet) {
        if unsafe { (*packet.pkt).stream_index } == video_stream.idx {
            i += 1;
        } else if unsafe { (*packet.pkt).stream_index } == audio_stream.idx {
            AVPacketReader::new(&packet)
                .read_to_end(&mut thd_vec)
                .unwrap();
        }
    }

    return Some(i);
}
