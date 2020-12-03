use super::{
    AVCodecContext, AVError, AVFrame, AVPacket, AVStream, MediaDuration, SwrContext, SwrOptions,
    VideoMetadata,
};
use std::{
    convert::TryInto,
    fmt::Display,
    ops::{Add, AddAssign},
};

pub struct ThdDecodePacket {
    pub original: DecodedThdFrame,
    pub mono: DecodedThdFrame,
}

/// Decodes the given packets from the stream.
pub fn decode(stream: &AVStream, packets: Vec<AVPacket>) -> Result<Vec<ThdDecodePacket>, AVError> {
    if stream.codec.id != ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD {
        panic!("attempted to decode a non-TrueHD stream.");
    }

    if packets.is_empty() {
        return Ok(Vec::new());
    }

    // allocate an AVFrame which will be filled with the decoded audio frame
    let mut av_frame = AVFrame::new();

    let mut a_ctx = stream.get_codec_context()?;
    a_ctx.open(&stream)?;

    let mut frame_buf = Vec::with_capacity(packets.len());
    for packet in packets {
        a_ctx.decode_frame(&packet, &mut av_frame)?;
        let decoded_frame = DecodedThdFrame::from(&av_frame);
        let mono_frame = downmix_mono(&av_frame, &a_ctx)?;
        let decoded_mono_frame = DecodedThdFrame::from(&mono_frame);

        frame_buf.push(ThdDecodePacket {
            original: decoded_frame,
            mono: decoded_mono_frame,
        });
    }

    Ok(frame_buf)
}

pub fn downmix_mono<'a>(
    frame: &'a AVFrame,
    codec_ctx: &AVCodecContext,
) -> Result<AVFrame<'a>, AVError> {
    let opts = SwrOptions {
        out_ch_layout: ffmpeg4_ffi::sys::AV_CH_LAYOUT_MONO as i64,
        out_sample_rate: codec_ctx.ctx.sample_rate,
        out_sample_fmt: codec_ctx.ctx.sample_fmt,
        in_ch_layout: unsafe {
            ffmpeg4_ffi::sys::av_get_default_channel_layout(codec_ctx.ctx.channels)
        },
        in_sample_rate: codec_ctx.ctx.sample_rate,
        in_sample_fmt: codec_ctx.ctx.sample_fmt,
    };
    let mut au_convert_ctx = SwrContext::with_options(&opts).unwrap();
    let output_frame = au_convert_ctx.convert_frame(&frame);
    Ok(output_frame)
}

/// A very light-weight header that only contains a length and a flag of whether
/// the encoded frame contains a major sync header.
pub struct ThdFrameHeader {
    pub length: usize,
    pub has_major_sync: bool,
}

impl ThdFrameHeader {
    pub fn from_bytes(bytes: &[u8]) -> Option<ThdFrameHeader> {
        if bytes.len() < 8 {
            return None;
        }

        let access_unit_length = {
            let bytes = [(bytes[0] & 0x0f), bytes[1]];
            (u16::from_be_bytes(bytes) as usize) * 2
        };
        let has_major_sync = {
            let major_sync_flag = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
            major_sync_flag == 0xf8726fba
        };

        return Some(ThdFrameHeader {
            length: access_unit_length,
            has_major_sync,
        });
    }
}

#[derive(Debug)]
pub struct ThdSample {
    // TODO: half the size of it by packing the 24 bit data with the 8 bit channel number
    // most significant byte (native endianness) contains the channel number as a u8,
    // least three significant bytes contain the 24-bit audio sample
    // packed: u32
    pub value: i32,
    pub channel: u8,
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

#[derive(Debug, Copy, Clone)]
pub struct ThdMetadata {
    pub channels: u8,
    pub sample_rate: u32,
    pub frame_size: u8,
}

#[derive(Debug)]
pub struct DecodedThdFrame {
    pub samples: Vec<ThdSample>,
    pub metadata: ThdMetadata,
}

impl Display for DecodedThdFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Format: {}", self.metadata)?;
        for i in 0..self.metadata.channels {
            let samples: Vec<i32> = self
                .samples
                .iter()
                .filter_map(|f| if f.channel == i { Some(f.value) } else { None })
                .collect();
            writeln!(f, "ch {}, samples: {:?}", i, samples)?;
        }
        Ok(())
    }
}

impl Display for ThdMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} Hz, {} ch", self.sample_rate, self.channels)
    }
}

impl DecodedThdFrame {
    pub fn new(
        bytes: &[u8],
        bytes_per_sample: usize,
        channels: u8,
        sample_rate: u32,
    ) -> DecodedThdFrame {
        let samples: Vec<ThdSample> = bytes
            .chunks_exact(bytes_per_sample)
            .enumerate()
            .map(|(i, c)| {
                ThdSample::from_bytes(c.try_into().unwrap(), (i % channels as usize) as u8)
            })
            .collect();
        DecodedThdFrame {
            samples,
            metadata: ThdMetadata {
                channels,
                sample_rate,
                frame_size: (sample_rate / 1200) as u8,
            },
        }
    }
}

pub struct ThdSegment {
    pub last_group_of_frames: Vec<(ThdDecodePacket, ThdFrameHeader)>,
    pub num_frames: u32,
    pub num_video_frames: u32,
    pub thd_metadata: ThdMetadata,
    pub video_metadata: VideoMetadata,
}

impl ThdSegment {
    pub fn overrun(&self) -> f64 {
        self.thd_metadata.duration(self.num_frames)
            - self.video_metadata.duration(self.num_video_frames)
    }
}

impl MediaDuration for ThdMetadata {
    fn duration(&self, frames: u32) -> f64 {
        (frames * self.frame_size as u32) as f64 / self.sample_rate as f64
    }
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
