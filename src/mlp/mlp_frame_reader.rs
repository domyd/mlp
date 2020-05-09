use super::MlpFrame;
use std::io::{Read, Seek, SeekFrom};

pub struct MlpFrameReader<'frame, 'reader, R: Read + Seek> {
    frame: &'frame MlpFrame,
    bytes_read: usize,
    reader: &'reader mut R,
}

impl<'frame, 'reader, R: Read + Seek> MlpFrameReader<'frame, 'reader, R> {
    pub fn new(
        frame: &'frame MlpFrame,
        reader: &'reader mut R,
    ) -> MlpFrameReader<'frame, 'reader, R> {
        MlpFrameReader {
            frame,
            bytes_read: 0,
            reader,
        }
    }
}

impl<'frame, 'reader, R: Read + Seek> Read for MlpFrameReader<'frame, 'reader, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let pos = self.bytes_read + self.frame.offset;
        let seek_from = SeekFrom::Start(pos as u64);
        self.reader.seek(seek_from)?;

        let left_to_read = self.frame.length - self.bytes_read;
        if left_to_read <= 0 {
            return Ok(0);
        }

        let read_at_most = std::cmp::min(left_to_read, buf.len());

        let bytes_read = self.reader.read(&mut buf[..read_at_most])?;
        self.bytes_read += bytes_read;

        return Ok(bytes_read);
    }
}
