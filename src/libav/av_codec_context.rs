use super::{AVError, AVFrame, AVPacket, AVStream};
use ffmpeg4_ffi::sys as ff;

pub struct AVCodecContext<'a> {
    pub ctx: &'a mut ff::AVCodecContext,
}

impl AVCodecContext<'_> {
    pub fn new<'a>(stream: &'a AVStream) -> Result<AVCodecContext<'a>, AVError> {
        let codec_ctx = unsafe { ff::avcodec_alloc_context3(stream.codec).as_mut() }
            .expect("ffmpeg failed to allocate codec context (avcodec_alloc_context3).");

        match unsafe { ff::avcodec_parameters_to_context(codec_ctx, stream.codec_params) } {
            i if i < 0 => Err(AVError::FFMpegErr(i)),
            _ => Ok(AVCodecContext { ctx: codec_ctx }),
        }
    }

    pub fn open(&mut self, stream: &AVStream) -> Result<(), AVError> {
        match unsafe { ff::avcodec_open2(&mut *self.ctx, stream.codec, std::ptr::null_mut()) } {
            0 => Ok(()),
            i if i < 0 => Err(AVError::FFMpegErr(i)),
            i => panic!(
                "avcodec_open2 returned {}, which is undocumented behavior.",
                i
            ),
        }
    }

    pub fn decode_frame(&mut self, packet: &AVPacket, frame: &mut AVFrame) -> Result<(), AVError> {
        self.send_packet(&packet)?;
        self.recv_frame(frame)?;
        Ok(())
    }

    pub fn send_packet(&mut self, packet: &AVPacket) -> Result<(), AVError> {
        unsafe {
            match ff::avcodec_send_packet(&mut *self.ctx, &packet.pkt) {
                0 => Ok(()),
                i if i < 0 => Err(AVError::FFMpegErr(i)),
                i => panic!(
                    "avcodec_send_packet returned {}, which is undocumented behavior.",
                    i
                ),
            }
        }
    }

    pub fn recv_frame(&mut self, frame: &mut AVFrame) -> Result<(), AVError> {
        match unsafe { ff::avcodec_receive_frame(&mut *self.ctx, frame.frame) } {
            0 => Ok(()),
            i if i < 0 => Err(AVError::FFMpegErr(i)),
            i => panic!(
                "avcodec_receive_frame returned {}, which is undocumented behavior.",
                i
            ),
        }
    }
}

impl Drop for AVCodecContext<'_> {
    fn drop(&mut self) {
        unsafe {
            let mut ctx: *mut ff::AVCodecContext = &mut *self.ctx;
            ff::avcodec_free_context(&mut ctx);
        }
    }
}
