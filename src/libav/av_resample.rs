use super::{AVError, AVFrame};
use ffmpeg4_ffi::sys as ff;

pub struct SwrContext<'a> {
    pub ctx: &'a mut ff::SwrContext,
}

pub struct SwrOptions {
    pub out_ch_layout: i64,
    pub out_sample_fmt: ff::AVSampleFormat,
    pub out_sample_rate: i32,
    pub in_ch_layout: i64,
    pub in_sample_fmt: ff::AVSampleFormat,
    pub in_sample_rate: i32,
}

impl SwrContext<'_> {
    pub fn new() -> Self {
        let ctx = unsafe { ff::swr_alloc().as_mut() }
            .expect("ffmpeg failed to allocate resample context (swr_alloc).");
        SwrContext { ctx }
    }

    pub fn with_options(opts: &SwrOptions) -> Result<SwrContext, AVError> {
        let ctx = unsafe {
            ff::swr_alloc_set_opts(
                std::ptr::null_mut(),
                opts.out_ch_layout,
                opts.out_sample_fmt,
                opts.out_sample_rate,
                opts.in_ch_layout,
                opts.in_sample_fmt,
                opts.in_sample_rate,
                0,
                std::ptr::null_mut(),
            )
            .as_mut()
        }
        .expect("ffmpeg failed to allocate resample context (swr_alloc_set_opts).");
        match unsafe { ff::swr_init(ctx) } {
            i if i < 0 => Err(AVError::FFMpegErr(i)),
            _ => Ok(SwrContext { ctx }),
        }
    }

    pub fn convert_frame<'i, 'o>(&mut self, input_frame: &'i AVFrame) -> AVFrame<'o> {
        let mut output_frame = AVFrame::new();
        output_frame.frame.channel_layout = ff::AV_CH_LAYOUT_MONO as u64;
        output_frame.frame.sample_rate = input_frame.frame.sample_rate;
        output_frame.frame.format = input_frame.frame.format;
        unsafe {
            ff::swr_convert_frame(self.ctx, output_frame.frame, input_frame.frame);
        }
        output_frame
    }
}

impl Drop for SwrContext<'_> {
    fn drop(&mut self) {
        unsafe {
            let mut ctx: *mut ff::SwrContext = &mut *self.ctx;
            ff::swr_free(&mut ctx);
        }
    }
}
