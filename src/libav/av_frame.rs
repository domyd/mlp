use ffmpeg4_ffi::sys as ff;

pub struct AVFrame {
    pub frame: *mut ff::AVFrame,
}

impl AVFrame {
    pub fn new() -> AVFrame {
        match unsafe { ff::av_frame_alloc().as_mut() } {
            Some(frame) => AVFrame {
                frame: frame as *mut ff::AVFrame,
            },
            None => panic!("ffmpeg failed to allocate AVFrame."),
        }
    }

    pub fn len(&self) -> usize {
        // todo: REALLY should be sure here that we're dealing with an audio frame
        // this is (probably) bonkers wrong with video frames

        let n_samples = self.samples() as usize;
        let n_channels = self.channels() as usize;
        let bytes_per_sample = self.bytes_per_sample();

        n_samples * n_channels * bytes_per_sample
    }

    pub fn bytes_per_sample(&self) -> usize {
        unsafe { ff::av_get_bytes_per_sample((*self.frame).format) as usize }
    }

    pub fn sample_rate(&self) -> u32 {
        unsafe { (*self.frame).sample_rate as u32 }
    }

    pub fn channels(&self) -> u8 {
        unsafe { (*self.frame).channels as u8 }
    }

    pub fn samples(&self) -> u32 {
        unsafe { (*self.frame).nb_samples as u32 }
    }

    pub fn as_slice<'a>(&'a self) -> &'a [u8] {
        let data_slice: &'a [u8] =
            unsafe { std::slice::from_raw_parts((*self.frame).data[0], self.len()) };

        data_slice
    }
}

impl Drop for AVFrame {
    fn drop(&mut self) {
        unsafe {
            ff::av_frame_free(&mut self.frame);
        }
    }
}
