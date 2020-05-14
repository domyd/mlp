use super::{AVError, AVFrame, AVPacket, AVStream};
use ffmpeg4_ffi::sys as ff;

pub struct AVCodecContext {
    pub ctx: *mut ff::AVCodecContext,
}

impl AVCodecContext {
    pub fn new(stream: &AVStream) -> Result<AVCodecContext, AVError> {
        let codec_ctx = unsafe { ff::avcodec_alloc_context3(stream.codec) };
        if codec_ctx.is_null() {
            return Err(AVError::FFMpegErr(-1));
        }

        let err = unsafe { ff::avcodec_parameters_to_context(codec_ctx, stream.codec_params) };
        if err != 0 {
            return Err(AVError::FFMpegErr(err));
        }

        return Ok(AVCodecContext { ctx: codec_ctx });
    }

    pub fn open(&self, stream: &AVStream) -> Result<(), AVError> {
        match unsafe { ff::avcodec_open2(self.ctx, stream.codec, std::ptr::null_mut()) } {
            0 => Ok(()),
            err => Err(AVError::FFMpegErr(err)),
        }
    }

    pub fn decode_frame(&mut self, packet: &AVPacket, frame: &mut AVFrame) -> Result<(), AVError> {
        self.send_packet(&packet)?;
        self.recv_frame(frame)?;
        Ok(())
    }

    pub fn send_packet(&mut self, packet: &AVPacket) -> Result<(), AVError> {
        unsafe {
            match ff::avcodec_send_packet(self.ctx, packet.pkt) {
                0 => Ok(()),
                err => Err(AVError::FFMpegErr(err)),
            }
        }
    }

    pub fn recv_frame(&mut self, frame: &mut AVFrame) -> Result<(), AVError> {
        match unsafe { ff::avcodec_receive_frame(self.ctx, frame.frame) } {
            0 => Ok(()),
            err => Err(AVError::FFMpegErr(err)),
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
