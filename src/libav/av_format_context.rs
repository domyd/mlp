use super::{AVCodecContext, AVError, AVPacket};
use ffmpeg4_ffi::sys as ff;
use std::path::Path;

pub struct AVFormatContext<'a> {
    ctx: &'a mut ff::AVFormatContext,
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

pub struct AVStream<'a> {
    pub stream: &'a ff::AVStream,
    pub codec: &'a ff::AVCodec,
    pub codec_params: &'a ff::AVCodecParameters,
}

impl AVStream<'_> {
    pub fn get_codec_context(&self) -> Result<AVCodecContext, AVError> {
        let ctx = AVCodecContext::new(self)?;
        Ok(ctx)
    }

    pub fn codec_type(&self) -> AVCodecType {
        match self.codec_params.codec_type {
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

impl AVFormatContext<'_> {
    pub fn open<P: AsRef<Path>>(input_path: P) -> Result<Self, AVError> {
        let input_path_cstr =
            std::ffi::CString::new(input_path.as_ref().to_str().unwrap()).unwrap();

        unsafe {
            let mut ctx_ptr: *mut _ = std::ptr::null_mut();
            let open_result = ff::avformat_open_input(
                &mut ctx_ptr,
                input_path_cstr.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if open_result == 0 && !ctx_ptr.is_null() {
                // correctly initialized
                Ok(AVFormatContext {
                    ctx: ctx_ptr.as_mut().unwrap(),
                })
            } else {
                Err(AVError::FFMpegErr(open_result))
            }
        }
    }

    pub fn streams<'ctx, 'stream: 'ctx>(&'ctx mut self) -> Result<Vec<AVStream<'stream>>, AVError> {
        let err = unsafe { ff::avformat_find_stream_info(&mut *self.ctx, std::ptr::null_mut()) };
        if err != 0 {
            return Err(AVError::FFMpegErr(err));
        }
        if self.ctx.streams.is_null() {
            panic!("AVFormatContext.streams was null.");
        }

        let av_streams = unsafe {
            std::slice::from_raw_parts((*self.ctx).streams, (*self.ctx).nb_streams as usize)
        };
        return Ok(av_streams
            .iter()
            .filter_map(|&stream| {
                let stream = unsafe { stream.as_ref() }?;
                let codec_params = unsafe { stream.codecpar.as_ref() }?;
                let codec = unsafe { ff::avcodec_find_decoder(codec_params.codec_id).as_ref() }?;
                return Some(AVStream {
                    stream,
                    codec,
                    codec_params,
                });
            })
            .collect());
    }

    pub fn read_frame(&mut self) -> Result<AVPacket, AVError> {
        let mut packet = AVPacket::new();
        match unsafe { ff::av_read_frame(self.ctx, &mut packet.pkt) } {
            0 => Ok(packet),
            err => Err(AVError::FFMpegErr(err)),
        }
    }
}

impl<'a> Drop for AVFormatContext<'a> {
    fn drop(&mut self) {
        unsafe {
            let mut ctx: *mut ff::AVFormatContext = &mut *self.ctx;
            ff::avformat_close_input(&mut ctx);
        }
    }
}
