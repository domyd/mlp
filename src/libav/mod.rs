use ffmpeg4_ffi::sys as ff;
use std::{
    convert::TryInto,
    io::{Seek, SeekFrom, Write},
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

pub struct ThdSample {
    // TODO: half the size of it by packing the 24 bit data with the 8 bit channel number
    // most significant byte (native endianness) contains the channel number as a u8,
    // least three significant bytes contain the 24-bit audio sample
    // packed: u32
    value: i32,
    channel: u8,
}

impl ThdSample {
    pub fn from_bytes(bytes: [u8; 4], channel: u8) -> Self {
        let sample = i32::from_ne_bytes(bytes) / 256;
        ThdSample::new(sample, channel)
    }

    pub fn new(sample: i32, channel: u8) -> Self {
        ThdSample {
            value: sample,
            channel,
        }
    }
}

pub struct DecodedThdFrame {
    samples: Vec<ThdSample>,
    num_channels: u8,
}

impl DecodedThdFrame {
    pub fn new(bytes: &[u8], bytes_per_sample: usize, channels: u8) -> DecodedThdFrame {
        let samples: Vec<ThdSample> = bytes
            .chunks_exact(bytes_per_sample)
            .enumerate()
            .map(|(i, c)| {
                ThdSample::from_bytes(c.try_into().unwrap(), (i % channels as usize) as u8)
            })
            .collect();
        DecodedThdFrame {
            samples,
            num_channels: channels,
        }
    }

    pub fn get_max_distance(&self, other: &Self) -> i32 {
        let mut max_distance: i32 = 0;
        for (l, r) in self.samples.iter().zip(other.samples.iter()) {
            let diff = (l.value - r.value).abs();
            max_distance = diff.max(max_distance);
        }
        max_distance
    }

    pub fn matches_approximately(&self, other: &Self, tolerance: i32) -> bool {
        for (l, r) in self.samples.iter().zip(other.samples.iter()) {
            let diff = (l.value - r.value).abs();
            if diff > tolerance {
                return false;
            }
        }
        return true;
    }

    pub fn is_silence(&self, threshold: i32) -> bool {
        self.samples.iter().all(|s| s.value.abs() < threshold)
    }
}

impl From<&AVFrame> for DecodedThdFrame {
    fn from(frame: &AVFrame) -> Self {
        let bytes = frame.as_slice();
        let bytes_per_sample = frame.bytes_per_sample();
        let channels = frame.channels();
        DecodedThdFrame::new(bytes, bytes_per_sample, channels)
    }
}

fn decode_thd(stream: &AVStream, packets: Vec<AVPacket>) -> Vec<DecodedThdFrame> {
    assert!(stream.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD);

    let mut av_frame = AVFrame::new().unwrap();

    let mut a_ctx = stream.get_codec_context().unwrap();
    a_ctx.open(&stream).unwrap();

    let mut frame_buf: Vec<DecodedThdFrame> = Vec::with_capacity(packets.len());
    for packet in packets {
        a_ctx.decode_frame(&packet, &mut av_frame).unwrap();
        let decoded_frame = DecodedThdFrame::from(&av_frame);
        frame_buf.push(decoded_frame);
    }

    frame_buf
}

fn print_frames(frames: &Vec<DecodedThdFrame>) {
    for sample in frames.iter().flat_map(|f| f.samples.iter()) {
        println!("ch[{}], s[{}]", sample.channel, sample.value)
    }
    // match  {
    //     Some(samples) => {
    //         for sample in samples {

    //         }
    //     }
    //     None => println!("no samples."),
    // }
}

fn read_thd_audio_tail(
    format_context: &AVFormatContext,
    stream: &AVStream,
) -> Option<DecodedThdFrame> {
    let mut packets: Vec<AVPacket> = Vec::with_capacity(128);

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        let thd_frame = ThdFrameHeader::from_bytes(&packet.as_slice()).unwrap();
        if thd_frame.has_major_sync {
            // clear the frame queue, new major sync is in town
            packets.truncate(0);
        }

        packets.push(packet);
    }

    decode_thd(stream, packets).pop()
}

fn read_thd_audio_head(
    format_context: &AVFormatContext,
    stream: &AVStream,
) -> Option<DecodedThdFrame> {
    let mut av_frame = AVFrame::new().unwrap();

    let mut a_ctx = stream.get_codec_context().unwrap();
    a_ctx.open(&stream).unwrap();

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        a_ctx.decode_frame(&packet, &mut av_frame).unwrap();
        let decoded_frame = DecodedThdFrame::from(&av_frame);
        return Some(decoded_frame);
    }

    return None;
}

pub fn thd_audio_read_test(file: &PathBuf, head: bool, threshold: i32) {
    let file_path_str = file.to_str().unwrap();
    println!("processing file '{}' ...", &file_path_str);
    let avctx = AVFormatContext::new(&file_path_str).unwrap();
    let streams = avctx.get_streams().unwrap();

    let audio_stream = streams
        .iter()
        .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
        .unwrap();

    if head {
        if let Some(frame) = read_thd_audio_head(&avctx, &audio_stream) {
            println!("is silence: {}", &frame.is_silence(threshold));
            print_frames(&vec![frame]);
        }
    } else {
        if let Some(frame) = read_thd_audio_tail(&avctx, &audio_stream) {
            println!("is silence: {}", &frame.is_silence(threshold));
            print_frames(&vec![frame]);
        }
    }
}

pub struct DemuxOptions {
    pub audio_match_threshold: i32,
}

pub fn demux_thd<W: Write + Seek>(
    files: &[PathBuf],
    options: &DemuxOptions,
    out_writer: &mut W,
) -> Result<ThdOverrun, AVError> {
    let mut overrun_acc = ThdOverrun { acc: 0.0 };
    let mut previous_segment: Option<ThdSegment> = None;

    for file in files {
        // check overrun and apply sync, if necessary
        if let Some(ref mut prev) = previous_segment {
            overrun_acc += prev.audio_overrun();

            // match audio data
            let (tail, tail_header) = prev.last_group_of_frames.last().unwrap();
            let head = {
                let file_path_str = file.to_str().unwrap();
                let avctx = AVFormatContext::new(&file_path_str).unwrap();
                let streams = avctx.get_streams().unwrap();
                let thd_stream = streams
                    .iter()
                    .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
                    .unwrap();
                read_thd_audio_head(&avctx, thd_stream)
            }
            .unwrap();

            if tail.is_silence(options.audio_match_threshold)
                || head.is_silence(options.audio_match_threshold)
            {
                // fall back to bresenham algorithm
                println!("AUDIO APPEARS TO BE SILENCE, FALLBACK TO OVERRUN MINIMIZATION");
                while overrun_acc.samples() >= 20 {
                    dbg!(overrun_acc.samples());
                    if let Some((_, header)) = prev.last_group_of_frames.last() {
                        // delete the most recently written frame by moving the cursor back
                        out_writer
                            .seek(SeekFrom::Current(-(header.length as i64)))
                            .unwrap();
                        overrun_acc.sub_frames(1);
                        println!("CUT FRAME");
                    }
                }
            } else {
                // do audio matching
                let max_distance = head.get_max_distance(tail);
                println!("max distance between tail and head: {}", max_distance);

                // Empirically, 256 seems like a fairly good tolerance value.
                // When the audio doesn't match, the max difference tends to
                // be in the hundreds of thousands, so there should be a good
                // bit of headroom in there.
                if head.matches_approximately(tail, 256) {
                    // delete the most recently written frame by moving the cursor back
                    out_writer
                        .seek(SeekFrom::Current(-(tail_header.length as i64)))
                        .unwrap();
                    overrun_acc.sub_frames(1);
                    println!("AUDIO MATCHES, CUT FRAME");
                } else {
                    println!("AUDIO DOESN'T MATCH, LEAVE FRAME");
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

        if let Ok(segment) = write_thd_segment(&avctx, video_stream, thd_stream, out_writer) {
            previous_segment = Some(segment);
        } else {
            println!("error happened while processing '{}'", file.display());
        }
    }

    return Ok(overrun_acc);
}

struct ThdSegment {
    //most_recent_frames: VecDeque<ThdFrameHeader>,
    last_group_of_frames: Vec<(DecodedThdFrame, ThdFrameHeader)>,
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

    // keeps the packets of the most recent group of frames
    // (all frames "belonging" to one major sync)
    let mut packet_queue: Vec<AVPacket> = Vec::with_capacity(128);

    // keeps track the last 10 frame headers we've written
    let mut frame_queue: Vec<ThdFrameHeader> = Vec::with_capacity(128);

    while let Ok(packet) = format_context.read_frame() {
        if packet.of_stream(video_stream) {
            // increase video frame counter (which we need in order to calculate
            // the precise video duration)
            num_video_frames += 1;
        } else if packet.of_stream(audio_stream) {
            // write the THD frame to the output
            let pkt_slice = packet.as_slice();
            thd_writer.write_all(&pkt_slice).unwrap();

            // push frame header to queue (we want to remember the last
            // n frame headers we saw)
            let frame = ThdFrameHeader::from_bytes(&pkt_slice).unwrap();
            if frame.has_major_sync {
                packet_queue.truncate(0);
                frame_queue.truncate(0);
            }
            packet_queue.push(packet);
            frame_queue.push(frame);

            // increase THD frame counter
            num_frames += 1;
        }
    }

    let decoded_frames = decode_thd(audio_stream, packet_queue)
        .into_iter()
        .zip(frame_queue.into_iter())
        .map(|(d, h)| (d, h))
        .collect();

    Ok(ThdSegment {
        last_group_of_frames: decoded_frames,
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
