use ffmpeg4_ffi::sys as ff;
use std::{
    collections::VecDeque,
    convert::TryInto,
    io::{Read, Seek, SeekFrom, Write},
    ops::{Add, AddAssign},
    path::PathBuf,
};

pub mod av_codec_context;
pub mod av_format_context;
pub mod av_frame;
pub mod av_packet;
pub use av_codec_context::AVCodecContext;
pub use av_format_context::{AVCodecType, AVFormatContext, AVStream};
pub use av_frame::AVFrame;
pub use av_packet::{AVPacket, AVPacketReader};

#[derive(Debug)]
pub enum AVError {
    IoErr(std::io::Error),
    FFMpegErr(i32),
}

pub struct ThdFrameHeader {
    pub length: usize,
    pub has_major_sync: bool,
}

impl ThdFrameHeader {
    pub fn from_bytes(bytes: &[u8]) -> Option<ThdFrameHeader> {
        if bytes.len() < 8 {
            return None;
        }

        let au_bytes = [(bytes[0] & 0x0f), bytes[1]];
        let access_unit_length = u16::from_be_bytes(au_bytes) * 2;
        assert_eq!(access_unit_length as usize, bytes.len());

        let has_major_sync = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) == 0xf8726fba;

        return Some(ThdFrameHeader {
            length: bytes.len(),
            has_major_sync,
        });
    }
}

fn read_thd_audio_tail(format_context: &AVFormatContext, stream: &AVStream) {
    let mut packet_queue: Vec<AVPacket> = Vec::with_capacity(128);
    let mut thd_buf: Vec<u8> = Vec::with_capacity(4096);

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        AVPacketReader::new(&packet)
            .read_to_end(&mut thd_buf)
            .unwrap();
        let thd_frame = ThdFrameHeader::from_bytes(&thd_buf).unwrap();
        if thd_frame.has_major_sync {
            // clear the frame queue, new major sync is in town
            packet_queue.truncate(0);
        }

        packet_queue.push(packet);
        thd_buf.truncate(0);
    }

    // we should now have the most recent THD group of frames in `packet_queue`.

    println!("packet queue has {} elements", packet_queue.len());
    assert!(packet_queue.len() >= 1);

    let mut av_frame = AVFrame::new().unwrap();

    let mut a_ctx = stream.get_codec_context().unwrap();
    a_ctx.open(&stream).unwrap();

    // decode first frame to get a sense of how much memory we'll need
    // to allocate for the audio buffer
    let mut packet_queue_iter = packet_queue.iter();
    let first_packet = packet_queue_iter.next().unwrap();
    a_ctx.decode_frame(&first_packet, &mut av_frame).unwrap();

    let bytes_per_sample = av_frame.bytes_per_sample();
    let num_channels = av_frame.channels();

    let mut audio_buf: Vec<u8> = Vec::with_capacity(av_frame.len());

    // write first frame
    audio_buf.write_all(av_frame.as_slice()).unwrap();

    for packet in packet_queue_iter {
        audio_buf.truncate(0);
        a_ctx.decode_frame(&packet, &mut av_frame).unwrap();
        audio_buf.write_all(av_frame.as_slice()).unwrap();
    }

    // audio_buf now contains the last audio frame of the stream

    println!("printing ch 0 samples ...");
    for (_, sample) in audio_buf
        .chunks_exact(bytes_per_sample)
        .map(|c| i32::from_ne_bytes(c.try_into().unwrap()) / 256)
        .enumerate()
        .filter(|(i, _)| i % (num_channels as usize) == 0)
    {
        println!("sample: {}", sample);
    }

    println!("done!");
}

fn read_thd_audio_head(format_context: &AVFormatContext, stream: &AVStream) {
    let mut av_frame = AVFrame::new().unwrap();
    let mut print_frames = 1;

    let mut a_ctx = stream.get_codec_context().unwrap();
    a_ctx.open(&stream).unwrap();

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        a_ctx.decode_frame(&packet, &mut av_frame).unwrap();

        let n_channels = av_frame.channels();
        let bytes_per_sample = av_frame.bytes_per_sample();

        // data is packed as data[sample][channel], in native endian order

        println!("printing ch 0 samples ...");
        for (_, sample) in av_frame
            .as_slice()
            .chunks_exact(bytes_per_sample)
            .map(|c| i32::from_ne_bytes(c.try_into().unwrap()) / 256)
            .enumerate()
            .filter(|(i, _)| i % (n_channels as usize) == 0)
        {
            println!("sample: {}", sample);
        }

        print_frames -= 1;
        if print_frames <= 0 {
            break;
        }
    }

    println!("done!");
}

pub fn thd_audio_read_test(file: &PathBuf, head: bool) {
    let file_path_str = file.to_str().unwrap();
    println!("processing file '{}' ...", &file_path_str);
    let avctx = AVFormatContext::new(&file_path_str).unwrap();
    let streams = avctx.get_streams().unwrap();

    let audio_stream = streams
        .iter()
        .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
        .unwrap();

    if head {
        read_thd_audio_head(&avctx, &audio_stream);
    } else {
        read_thd_audio_tail(&avctx, &audio_stream);
    }
}

pub fn concat_thd_from_m2ts<W: Write + Seek>(
    files: &[PathBuf],
    out_writer: &mut W,
) -> Result<ThdOverrun, AVError> {
    let mut overrun_acc = ThdOverrun { acc: 0.0 };
    let mut previous_segment: Option<ThdSegment> = None;

    for file in files {
        // check overrun and apply sync, if necessary
        if let Some(ref mut prev) = previous_segment {
            overrun_acc += prev.audio_overrun();
            while overrun_acc.samples() >= 20 {
                dbg!(overrun_acc.samples());
                if let Some(frame) = prev.most_recent_frames.pop_front() {
                    // delete the most recently written frame by moving the cursor back
                    out_writer
                        .seek(SeekFrom::Current(-(frame.length as i64)))
                        .unwrap();
                    overrun_acc.sub_frames(1);
                    println!("CUT FRAME");
                }
            }
        }

        // copy the thd segment to the writer
        let file_path_str = file.to_str().unwrap();
        println!("processing file '{}' ...", &file_path_str);
        dbg!(overrun_acc.samples());
        let avctx = AVFormatContext::new(&file_path_str).unwrap();
        let streams = avctx.get_streams().unwrap();

        let video_stream = streams
            .iter()
            .find(|&s| s.codec_type() == AVCodecType::Video)
            .unwrap();
        let thd_stream = streams
            .iter()
            .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
            .unwrap();

        if let Ok(segment) = write_thd_segment(
            &avctx,
            video_stream,
            thd_stream,
            out_writer,
        ) {
            previous_segment = Some(segment);
        } else {
            println!("error happened while processing '{}'", file.display());
        }
    }

    return Ok(overrun_acc);
}

struct VecDequeFixedSize<T> {
    vec: VecDeque<T>,
    max_size: usize,
}

impl<T> VecDequeFixedSize<T> {
    pub fn new(max_size: usize) -> VecDequeFixedSize<T> {
        VecDequeFixedSize {
            vec: VecDeque::with_capacity(max_size + 1),
            max_size,
        }
    }

    pub fn into_inner(self) -> VecDeque<T> {
        self.vec
    }

    pub fn push_front(&mut self, value: T) {
        self.vec.push_front(value);
        self.vec.truncate(self.max_size);
    }
}

struct ThdSegment {
    most_recent_frames: VecDeque<ThdFrameHeader>,
    num_frames: u32,
    num_video_frames: u32,
}

impl ThdSegment {
    pub fn audio_duration(&self) -> f64 {
        // we're assuming 48 KHz here, which is usually
        // true but doesn't have to be
        // TODO
        (self.num_frames * 40) as f64 / 48000_f64
    }

    pub fn video_duration(&self) -> f64 {
        // we're assuming 23.976 fps here, which isn't always true
        // TODO
        (self.num_video_frames * 1001) as f64 / 24000_f64
    }

    pub fn audio_overrun(&self) -> f64 {
        self.audio_duration() - self.video_duration()
    }
}

fn write_thd_segment<W: Write + Seek>(
    format_context: &AVFormatContext,
    video_stream: &AVStream,
    audio_stream: &AVStream,
    thd_writer: &mut W,
) -> Result<ThdSegment, AVError> {
    let (mut num_frames, mut num_video_frames) = (0u32, 0u32);

    // 4096 bytes should be large enough for any TrueHD frame
    let mut thd_buf: Vec<u8> = Vec::with_capacity(4096);
    let mut frame_queue: VecDequeFixedSize<ThdFrameHeader> = VecDequeFixedSize::new(10);

    while let Ok(packet) = format_context.read_frame() {
        if packet.of_stream(video_stream) {
            // increase video frame counter (which we need in order to calculate
            // the precise video duration)
            num_video_frames += 1;
        } else if packet.of_stream(audio_stream) {
            // write the THD frame to the output
            AVPacketReader::new(&packet)
                .read_to_end(&mut thd_buf)
                .unwrap();
            thd_writer.write_all(&thd_buf).unwrap();

            // push frame header to queue (we want to remember the last
            // n frame headers we saw)
            let frame = ThdFrameHeader::from_bytes(&thd_buf).unwrap();
            frame_queue.push_front(frame);

            // clear the audio buffer
            thd_buf.truncate(0);

            // increase THD frame counter
            num_frames += 1;
        }
    }

    Ok(ThdSegment {
        most_recent_frames: frame_queue.into_inner(),
        num_frames,
        num_video_frames,
    })
}

#[derive(Debug, Copy, Clone)]
pub struct ThdOverrun {
    pub acc: f64,
}

impl ThdOverrun {
    pub fn sub_frames(&mut self, frames: i32) {
        self.acc = self.acc - ((frames * 40) as f64 / 48000_f64);
    }

    pub fn samples(&self) -> i32 {
        (self.acc * 48000_f64).round() as i32
    }
}

impl Add for ThdOverrun {
    type Output = ThdOverrun;

    fn add(self, rhs: ThdOverrun) -> Self::Output {
        ThdOverrun {
            acc: self.acc + rhs.acc,
        }
    }
}

impl Add<f64> for ThdOverrun {
    type Output = ThdOverrun;
    fn add(self, rhs: f64) -> Self::Output {
        ThdOverrun {
            acc: self.acc + rhs,
        }
    }
}

impl AddAssign<f64> for ThdOverrun {
    fn add_assign(&mut self, rhs: f64) {
        self.acc += rhs;
    }
}
