use super::AVStream;
use ffmpeg4_ffi::sys as ff;
use std::mem::MaybeUninit;

pub struct AVPacket {
    pub pkt: ff::AVPacket,
}

impl AVPacket {
    /// Allocates a new `AVPacket` on the stack.
    pub fn new() -> AVPacket {
        let mut pkt = MaybeUninit::<ff::AVPacket>::uninit();
        unsafe {
            ff::av_init_packet(pkt.as_mut_ptr());
        }
        let pkt = unsafe { pkt.assume_init() };
        AVPacket { pkt }
    }

    pub fn stream_index(&self) -> i32 {
        self.pkt.stream_index
    }

    pub fn of_stream(&self, stream: &AVStream) -> bool {
        self.stream_index() == stream.stream.index
    }

    pub fn as_slice(&self) -> &[u8] {
        let slice: &[u8] =
            unsafe { std::slice::from_raw_parts(self.pkt.data, self.pkt.size as usize) };
        slice
    }

    pub fn data_len(&self) -> usize {
        self.pkt.size as usize
    }
}

impl Drop for AVPacket {
    fn drop(&mut self) {
        unsafe {
            ff::av_packet_unref(&mut self.pkt);
        }
    }
}
