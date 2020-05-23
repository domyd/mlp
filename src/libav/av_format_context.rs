use super::{AVCodecContext, AVError, AVPacket};
use ffmpeg4_ffi::sys as ff;
use std::path::Path;

pub struct AVFormatContext {
    ctx: *mut ff::AVFormatContext,
}

#[derive(PartialEq)]
pub enum AVCodecType {
    Unknown,
    Video,
    Audio,
    Data,
    Subtitle,
    Attachment,
}

pub struct AVStream {
    pub index: i32,
    pub codec: *mut ff::AVCodec,
    pub codec_params: *mut ff::AVCodecParameters,
}

impl AVStream {
    pub fn get_codec_context(&self) -> Result<AVCodecContext, AVError> {
        let ctx = AVCodecContext::new(self)?;
        Ok(ctx)
    }

    pub fn codec_id(&self) -> ff::AVCodecID {
        unsafe { (*self.codec).id }
    }

    pub fn codec_type(&self) -> AVCodecType {
        match unsafe { (*self.codec_params).codec_type } {
            ff::AVMediaType_AVMEDIA_TYPE_VIDEO => AVCodecType::Video,
            ff::AVMediaType_AVMEDIA_TYPE_AUDIO => AVCodecType::Audio,
            ff::AVMediaType_AVMEDIA_TYPE_DATA => AVCodecType::Data,
            ff::AVMediaType_AVMEDIA_TYPE_SUBTITLE => AVCodecType::Subtitle,
            ff::AVMediaType_AVMEDIA_TYPE_ATTACHMENT => AVCodecType::Attachment,
            _ => AVCodecType::Unknown,
        }
    }
}

pub struct AVCodecParameters {
    pub codec_params: *mut ff::AVCodecParameters,
    pub codec_type: AVCodecType,
}

impl AVFormatContext {
    pub fn new<P: AsRef<Path>>(input_path: P) -> Result<Self, AVError> {
        let input_path_cstr =
            std::ffi::CString::new(input_path.as_ref().to_str().unwrap()).unwrap();
        let mut avctx = unsafe { ff::avformat_alloc_context() };
        let open_result = unsafe {
            ff::avformat_open_input(
                &mut avctx,
                input_path_cstr.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if open_result == 0 {
            Ok(AVFormatContext { ctx: avctx })
        } else {
            Err(AVError::FFMpegErr(open_result))
        }
    }

    pub fn get_streams(&self) -> Result<Vec<AVStream>, AVError> {
        let err = unsafe { ff::avformat_find_stream_info(self.ctx, std::ptr::null_mut()) };
        if err != 0 {
            return Err(AVError::FFMpegErr(err));
        }
        if unsafe { (*self.ctx).streams.is_null() } {
            panic!("AVFormatContext.streams was null.");
        }

        let av_streams = unsafe {
            std::slice::from_raw_parts((*self.ctx).streams, (*self.ctx).nb_streams as usize)
        };
        return Ok(av_streams
            .iter()
            .enumerate()
            .filter_map(|(i, &stream)| {
                let codec_params: *mut ff::AVCodecParameters = unsafe { (*stream).codecpar };
                if codec_params.is_null() {
                    return None;
                }

                let codec: *mut ff::AVCodec =
                    unsafe { ff::avcodec_find_decoder((*codec_params).codec_id) };
                if codec.is_null() {
                    return None;
                }

                return Some(AVStream {
                    index: i as i32,
                    codec,
                    codec_params,
                });
            })
            .collect());
    }

    pub fn read_frame(&self) -> Result<AVPacket, AVError> {
        let mut packet = AVPacket::new();
        match unsafe { ff::av_read_frame(self.ctx, &mut packet.pkt) } {
            0 => Ok(packet),
            err => Err(AVError::FFMpegErr(err)),
        }
    }
}

impl Drop for AVFormatContext {
    fn drop(&mut self) {
        unsafe {
            ff::avformat_close_input(&mut self.ctx);
        }
    }
}
