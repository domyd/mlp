use ffmpeg4_ffi::sys as ff;
use ffmpeg4_ffi::sys::{
    AVMediaType_AVMEDIA_TYPE_AUDIO as AVMEDIA_TYPE_AUDIO,
    AVMediaType_AVMEDIA_TYPE_VIDEO as AVMEDIA_TYPE_VIDEO,
};
use std::{
    collections::VecDeque,
    convert::TryInto,
    io::{Read, Seek, SeekFrom, Write},
    ops::Add,
    path::PathBuf,
};

pub mod av_codec_context;
pub mod av_format_context;
pub mod av_frame;
pub mod av_packet;
pub use av_codec_context::AVCodecContext;
pub use av_format_context::AVFormatContext;
pub use av_frame::AVFrame;
pub use av_packet::{AVPacket, AVPacketReader};

#[derive(Debug)]
pub enum AVError {
    IoErr(std::io::Error),
    FFMpegErr(i32),
}

pub struct AVStream {
    idx: i32,
    codec: *mut ff::AVCodec,
    codec_params: *mut ff::AVCodecParameters,
}

impl AVStream {
    pub fn get_codec_context(&self) -> Result<AVCodecContext, AVError> {
        let ctx = AVCodecContext::new(self)?;
        Ok(ctx)
    }
}

fn is_true_hd_stream(stream: &AVStream) -> bool {
    unsafe {
        (*stream.codec).id == ff::AVCodecID_AV_CODEC_ID_TRUEHD
            && (*stream.codec_params).codec_type == ff::AVMediaType_AVMEDIA_TYPE_AUDIO
    }
}

pub struct ThdFrame {
    pub length: usize,
    pub input_timing: u16,
    pub has_major_sync: bool,
}

impl ThdFrame {
    pub fn from_bytes(bytes: &[u8]) -> Option<ThdFrame> {
        if bytes.len() < 8 {
            return None;
        }

        let au_bytes = [(bytes[0] & 0x0f), bytes[1]];
        let access_unit_length = u16::from_be_bytes(au_bytes) * 2;
        assert_eq!(access_unit_length as usize, bytes.len());

        let input_timing = u16::from_be_bytes([bytes[2], bytes[3]]);
        let has_major_sync = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) == 0xf8726fba;

        return Some(ThdFrame {
            length: bytes.len(),
            input_timing,
            has_major_sync,
        });
    }
}

fn read_thd_audio_tail(format_context: &AVFormatContext, stream: &AVStream) {
    let mut packet_queue: Vec<AVPacket> = Vec::with_capacity(128);
    let mut thd_buf: Vec<u8> = Vec::with_capacity(4096);

    // println!("bits per raw sample: {}", unsafe { (*a_ctx.ctx).bits_per_raw_sample });

    while let Ok(packet) = format_context.read_frame() {
        if unsafe { (*packet.pkt).stream_index } != stream.idx {
            continue;
        }

        AVPacketReader::new(&packet)
            .read_to_end(&mut thd_buf)
            .unwrap();
        let thd_frame = ThdFrame::from_bytes(&thd_buf).unwrap();
        if thd_frame.has_major_sync {
            // empty the frame queue, new major sync is in town
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

    println!("bits per raw sample: {}", unsafe {
        (*a_ctx.ctx).bits_per_raw_sample
    });

    while let Ok(packet) = format_context.read_frame() {
        if unsafe { (*packet.pkt).stream_index } != stream.idx {
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

    let audio_stream = streams.iter().find(|&s| is_true_hd_stream(s)).unwrap();

    if head {
        read_thd_audio_head(&avctx, &audio_stream);
    } else {
        read_thd_audio_tail(&avctx, &audio_stream);
    }
}

pub fn concat_thd_from_m2ts<W: Write + Seek>(
    files: &[PathBuf],
    out_writer: &mut W,
) -> Result<(usize, ThdOverrun), AVError> {
    let mut overrun_acc = ThdOverrun { acc: 0.0 };
    let mut bytes_written: usize = 0;

    for file in files {
        let file_path_str = file.to_str().unwrap();
        println!("processing file '{}' ...", &file_path_str);
        let avctx = AVFormatContext::new(&file_path_str).unwrap();
        let streams = avctx.get_streams().unwrap();

        let video_stream = streams
            .iter()
            .find(|&s| unsafe { (*s.codec_params).codec_type == AVMEDIA_TYPE_VIDEO })
            .unwrap();
        let audio_stream = streams.iter().find(|&s| is_true_hd_stream(s)).unwrap();

        let res = write_thd_segment(&avctx, video_stream, audio_stream, overrun_acc, out_writer)?;
        overrun_acc = res.1;
        bytes_written += res.0;
    }

    return Ok((bytes_written, overrun_acc));
}

pub fn write_thd_segment<W: Write + Seek>(
    format_context: &AVFormatContext,
    video_stream: &AVStream,
    audio_stream: &AVStream,
    thd_overrun: ThdOverrun,
    thd_writer: &mut W,
) -> Result<(usize, ThdOverrun), AVError> {
    let (mut n_thd_frames, mut n_video_frames) = (0u32, 0u32);
    let mut n_bytes_written: usize = 0;

    // 4096 bytes should be large enough for any TrueHD frame
    let mut thd_buf: Vec<u8> = Vec::with_capacity(4096);
    let mut frame_queue: VecDeque<ThdFrame> = VecDeque::with_capacity(11);

    while let Ok(packet) = format_context.read_frame() {
        if unsafe { (*packet.pkt).stream_index } == video_stream.idx {
            n_video_frames += 1;
        } else if unsafe { (*packet.pkt).stream_index } == audio_stream.idx {
            {
                let a_ctx = audio_stream.get_codec_context().unwrap();
                a_ctx.open(&audio_stream).unwrap();
            }
            n_bytes_written += AVPacketReader::new(&packet)
                .read_to_end(&mut thd_buf)
                .unwrap();
            thd_writer.write_all(&thd_buf).unwrap();
            let frame = ThdFrame::from_bytes(&thd_buf).unwrap();
            frame_queue.push_front(frame);
            frame_queue.truncate(10);
            thd_buf.truncate(0);
            n_thd_frames += 1;
        }
    }

    // compute the overrun
    let audio_duration: f64 = (n_thd_frames * 40) as f64 / 48000_f64;
    let video_duration: f64 = (n_video_frames * 1001) as f64 / 24000_f64;

    let mut overrun = thd_overrun
        + ThdOverrun {
            acc: audio_duration - video_duration,
        };
    dbg!(&overrun);

    while overrun.samples() >= 20 {
        if let Some(frame) = frame_queue.pop_front() {
            // delete the most recently written frame by moving the cursor back
            thd_writer
                .seek(SeekFrom::Current(-(frame.length as i64)))
                .unwrap();
            n_bytes_written -= frame.length;
            overrun.sub_frames(1);
            println!("CUT FRAME");
        }
    }

    dbg!(overrun);

    Ok((n_bytes_written, overrun))
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
