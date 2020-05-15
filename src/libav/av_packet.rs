use super::{AVError, AVStream};
use ffmpeg4_ffi::sys as ff;
use std::io::Read;

pub struct AVPacket {
    pub pkt: *mut ff::AVPacket,
}

impl AVPacket {
    pub fn new() -> Result<AVPacket, AVError> {
        let pkt = unsafe { ff::av_packet_alloc() };
        if pkt.is_null() {
            Err(AVError::FFMpegErr(1))
        } else {
            Ok(AVPacket { pkt })
        }
    }

    pub fn stream_index(&self) -> i32 {
        unsafe { (*self.pkt).stream_index }
    }

    pub fn of_stream(&self, stream: &AVStream) -> bool {
        self.stream_index() == stream.index
    }
}

impl Drop for AVPacket {
    fn drop(&mut self) {
        unsafe {
            ff::av_packet_unref(self.pkt);
            ff::av_packet_free(&mut self.pkt);
        }
    }
}

pub struct AVPacketReader<'packet> {
    data_slice: &'packet [u8],
}

impl<'packet> AVPacketReader<'_> {
    pub fn new(packet: &'packet AVPacket) -> AVPacketReader<'packet> {
        let slice: &'packet [u8] =
            unsafe { std::slice::from_raw_parts((*packet.pkt).data, (*packet.pkt).size as usize) };
        AVPacketReader { data_slice: slice }
    }
}

impl<'packet> Read for AVPacketReader<'packet> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.data_slice.read(buf)
    }
}
