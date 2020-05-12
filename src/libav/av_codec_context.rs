use super::{AVError, AVStream};
use ffmpeg4_ffi::sys as ff;

pub struct AVCodecContext {
    ctx: *mut ff::AVCodecContext,
}

impl AVCodecContext {
    pub fn new(stream: &AVStream) -> Result<AVCodecContext, AVError> {
        let codec_ctx = unsafe { ff::avcodec_alloc_context3(stream.codec) };
        if codec_ctx.is_null() {
            return Err(AVError { err: -1 });
        }

        let err = unsafe { ff::avcodec_parameters_to_context(codec_ctx, stream.codec_params) };
        if err != 0 {
            return Err(AVError { err });
        }

        return Ok(AVCodecContext { ctx: codec_ctx });
    }

    pub fn open(&self, stream: &AVStream) -> Result<(), AVError> {
        match unsafe { ff::avcodec_open2(self.ctx, stream.codec, std::ptr::null_mut()) } {
            0 => Ok(()),
            err => Err(AVError { err }),
        }
    }
}

impl Drop for AVCodecContext {
    fn drop(&mut self) {
        unsafe {
            ff::avcodec_free_context(&mut self.ctx);
        }
    }
}