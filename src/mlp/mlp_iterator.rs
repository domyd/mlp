use super::MlpFrame;
use std::{convert::TryInto, io::Read};

pub struct MlpIterator<R: Read> {
    buffer: Vec<u8>,
    reader: R,
    segment: u16,
    offset: usize,
}

impl<'a, R: Read> MlpIterator<R> {
    pub fn new(reader: R) -> MlpIterator<R> {
        MlpIterator::with_segment(reader, 0)
    }

    pub fn with_segment(reader: R, segment: u16) -> MlpIterator<R> {
        let buffer: Vec<u8> = vec![0; 4096];
        MlpIterator {
            buffer,
            reader,
            segment,
            offset: 0,
        }
    }
}

impl<R: Read> Iterator for MlpIterator<R> {
    type Item = MlpFrame;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.reader.read_exact(&mut self.buffer[..8]);
        if r.is_err() {
            return None;
        }

        let au_bytes = [(self.buffer[0] & 0x0f), self.buffer[1]];
        let access_unit_length = u16::from_be_bytes(au_bytes) * 2;
        let input_timing = u16::from_be_bytes([self.buffer[2], self.buffer[3]]);
        let has_major_sync = u32::from_be_bytes([
            self.buffer[4],
            self.buffer[5],
            self.buffer[6],
            self.buffer[7],
        ]) == 0xf8726fba;

        let au_len: usize = access_unit_length.try_into().unwrap();

        if self.buffer.capacity() < au_len {
            // resize
            let new_capacity = au_len.next_power_of_two();
            self.buffer.resize(new_capacity, 0);
        }

        match self.reader.read_exact(&mut self.buffer[..(au_len - 8)]) {
            Ok(()) => {
                let frame = MlpFrame {
                    segment: self.segment,
                    offset: self.offset,
                    length: au_len,
                    input_timing,
                    has_major_sync,
                };
                self.offset += au_len;
                Some(frame)
            }
            Err(_) => None,
        }
    }
}

mod tests {
    use super::MlpIterator;

    // depends on local assets that I obviously can't check into source control
    //#[test]
    #[allow(dead_code)]
    fn iter_test_full() {
        let iter = get_iter("assets/monstersuniversity-segment86-full.thd");

        let mut num_major_frames = 0;
        let mut num_frames = 0;
        for frame in iter {
            if frame.has_major_sync {
                num_major_frames += 1;
            }
            num_frames += 1;
        }
        assert_eq!(17018, num_frames);
        assert_eq!(135, num_major_frames);
    }

    #[test]
    fn iter_test_single() {
        let iter = get_iter("assets/truehd-major-frame.bin");
        let num_frames = iter.count();
        assert_eq!(1, num_frames);
    }

    #[test]
    fn iter_test_none() {
        let data: &[u8] = &[];
        let iterator = MlpIterator::new(data);
        let num_frames = iterator.count();
        assert_eq!(0, num_frames);
    }

    #[test]
    fn iter_test_partial() {
        let data: &[u8] = include_bytes!("../../assets/truehd-major-frame.bin");
        let iterator = MlpIterator::new(&data[..500]);
        let num_frames = iterator.count();
        assert_eq!(0, num_frames);
    }

    fn get_iter<P: AsRef<std::path::Path>>(
        relative_path: P,
    ) -> MlpIterator<std::io::BufReader<std::fs::File>> {
        let path: std::path::PathBuf = {
            let dir = env!("CARGO_MANIFEST_DIR");
            let mut path_buf = std::path::PathBuf::from(dir);
            path_buf.push(relative_path);
            path_buf
        };

        let file = std::fs::File::open(path).unwrap();
        let reader = std::io::BufReader::new(file);
        let iterator = MlpIterator::new(reader);
        iterator
    }
}
