use super::AVError;
use ffmpeg4_ffi::sys as ff;
use std::io::Read;

pub struct AVPacket {
    pub pkt: *mut ff::AVPacket,
}

impl AVPacket {
    pub fn new() -> Result<AVPacket, AVError> {
        let pkt = unsafe { ff::av_packet_alloc() };
        if pkt.is_null() {
            Err(AVError { err: 1 })
        } else {
            Ok(AVPacket { pkt })
        }
    }
}

impl Drop for AVPacket {
    fn drop(&mut self) {
        unsafe {
            ff::av_packet_unref(self.pkt);
        }
    }
}

pub struct AVPacketReader<'packet> {
    packet: &'packet AVPacket,
    data_slice: &'packet [u8],
}

impl<'packet> AVPacketReader<'_> {
    pub fn new(packet: &'packet AVPacket) -> AVPacketReader {
        let slice: &'packet [u8] =
            unsafe { std::slice::from_raw_parts((*packet.pkt).data, (*packet.pkt).size as usize) };
        AVPacketReader {
            packet,
            data_slice: slice,
        }
    }
}

impl<'packet> Read for AVPacketReader<'packet> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.data_slice.read(buf)
    }
}
