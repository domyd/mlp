use nom::{
    bytes::streaming::take,
    combinator::{opt, peek},
    do_parse,
    error::ErrorKind,
    named,
    number::streaming::{be_u16, be_u32, be_u64},
    peek,
    sequence::{tuple, Tuple},
    switch, tag, take, IResult, Slice,
};

struct TempAccessUnit {
    length: u16,
    input_timing: u16,
    is_major_sync: bool,
}

struct AccessUnit {
    sync_header: SyncHeader,
}

#[derive(Debug, PartialEq)]
struct MajorSyncInfo {
    format_info: FormatInfo,
    flags: u16,
    variable_rate: bool,
    /// The bit stream's peak data rate, given in the unit of 1/16 bits/sample period.
    /// Multiply this value by `sampling_frequency`/16 to get a value in bits per second.
    peak_data_rate: u16,
    substreams: u8,
    extended_substream_info: u8,
    substream_info: u8,
    channel_meaning: ChannelMeaning,
    crc: u16,
}

impl MajorSyncInfo {
    fn peak_data_rate_bps(&self) -> f32 {
        let sampling_frequency = self.format_info.sampling_frequency.value();
        let factor = (sampling_frequency as f32) / 16f32;
        (self.peak_data_rate as f32) * factor
    }
}

#[derive(Debug, PartialEq)]
struct FormatInfo {
    sampling_frequency: SamplingFrequency,
    six_ch_multichannel_type: MultichannelType,
    eight_ch_multichannel_type: MultichannelType,
    // todo: split up more
    channel_information: u32,
}

#[derive(Debug, PartialEq)]
enum MultichannelType {
    StandardLoudspeakerLayout,
    Unknown,
}

#[derive(Debug, PartialEq)]
#[allow(non_camel_case_types)]
enum SamplingFrequency {
    k48,
    k96,
    k192,
    k44_1,
    k88_2,
    k176_4,
    Unknown,
}

impl SamplingFrequency {
    fn value(&self) -> u32 {
        match (self) {
            SamplingFrequency::k48 => 48_000,
            SamplingFrequency::k96 => 96_000,
            SamplingFrequency::k192 => 192_000,
            SamplingFrequency::k44_1 => 44_100,
            SamplingFrequency::k88_2 => 88_200,
            SamplingFrequency::k176_4 => 176_400,
            Unknown => 1,
        }
    }
}

#[derive(Debug, PartialEq)]
struct SyncHeader {
    // Total length of the complete access unit, in bytes.
    access_unit_length: u16,
    // The time at which the access unit is input to the decoder, expressed in
    // sample periods and modulo 65536.
    input_timing: u16,
    major_sync_info: Option<MajorSyncInfo>,
}

#[derive(Debug, PartialEq)]
struct ControlEnabled {
    two_ch: bool,
    six_ch: bool,
    eight_ch: bool,
}

#[derive(Debug, PartialEq)]
struct DialNorm {
    // ranges between -1 LKFS and -63 LKFS
    two_ch: i8,
    // ranges between -1 LKFS and -31 LKFS
    six_ch: i8,
    // ranges between -1 LKFS and -31 LKFS
    eight_ch: i8,
}

#[derive(Debug, PartialEq)]
struct MixLevel {
    // ranges between 70 dB and 133 dB
    two_ch: u8,
    // ranges between 70 dB and 133 dB
    six_ch: u8,
    // ranges between 70 dB and 133 dB
    eight_ch: u8,
}

#[derive(Debug, PartialEq)]
struct ChSourceFormat {
    // not sure what this is, default value is 0x00000b
    six_ch: u8,
    // not sure what this is, default value is 0x00000b
    eight_ch: u8,
}

#[derive(Debug, PartialEq)]
struct ChannelMeaning {
    ctrl_enabled: ControlEnabled,
    dial_norm: DialNorm,
    mix_level: MixLevel,
    source_format: ChSourceFormat,
    drc_start_up_gain: i8,
    extra_channel_meaning: Option<ExtraChannelMeaning>,
}

#[derive(Debug, PartialEq)]
struct ExtraChannelMeaning {
    length: u8,
}

struct Substream {
    info: SubstreamInfo,
    parity: Option<u8>,
    crc: Option<u8>,
}

struct SubstreamInfo {
    restart_nonexistent: bool,
    crc_present: bool,
    substream_end_ptr: u16,
    extra_substream_word: Option<u16>,
}

// workaround: https://github.com/Geal/nom/issues/1036
fn bits_tuple<I, O, L>(l: L) -> impl Fn(I) -> IResult<I, O>
where
    I: Slice<std::ops::RangeFrom<usize>> + Clone,
    L: Tuple<(I, usize), O, ((I, usize), ErrorKind)>,
{
    nom::bits::bits(nom::sequence::tuple::<_, _, ((I, usize), ErrorKind), _>(l))
}

fn nibble_and_au_length(input: &[u8]) -> IResult<&[u8], (u8, u16)> {
    bits_tuple((
        nom::bits::streaming::take(4u8),
        nom::bits::streaming::take(12u8),
    ))(input)
}

// fn minor_sync_crc_check(input: &[u8]) -> IResult<&[u8], (bool, u8)> {
//     let nibbles = peek(bits_tuple(
//         nom::bits::streaming::take(4u8),
//         nom::bits::streaming::take(4u8),
//         nom::bits::streaming::take(4u8),
//     ));
// }

// fn channel_meaning(input: &[u8]) -> IResult<&[u8], ()

// fn major_sync(input: &[u8]) -> IResult<&[u8], SyncInfo> {
//     let is_major_sync = peek(nom::bits::streaming::tag(0xF8726FBA, 32));
//     if is_major_sync(input) {
//         Ok((input, SyncInfo::Minor))
//     }
// }

// fn access_unit(input: &[u8]) -> IResult<&[u8], TempAccessUnit> {
//     let (rest, (_, length)) = nibble_and_au_length(input)?;
//     bits_tuple((
//         nom::bits::streaming::take(16u8),
//         nom::bits::streaming::tag(0xF8726FBA, 32usize),
//     ))(rest)?;

//     Ok((
//         rest,
//         TempAccessUnit {
//             length,
//             is_major_sync: false,
//         },
//     ))
// }

fn format_info(input: &[u8]) -> IResult<&[u8], FormatInfo> {
    let (rest, flags) = be_u32(input)?;
    Ok((
        rest,
        FormatInfo {
            sampling_frequency: match (flags & 0xF0_00_00_00) >> 28 {
                0b0000 => SamplingFrequency::k48,
                0b0001 => SamplingFrequency::k96,
                0b0010 => SamplingFrequency::k192,
                0b1000 => SamplingFrequency::k44_1,
                0b1001 => SamplingFrequency::k88_2,
                0b1010 => SamplingFrequency::k176_4,
                _ => SamplingFrequency::Unknown,
            },
            six_ch_multichannel_type: match (flags & 0x08_00_00_00) >> 27 {
                0b0 => MultichannelType::StandardLoudspeakerLayout,
                _ => MultichannelType::Unknown,
            },
            eight_ch_multichannel_type: match (flags & 0x04_00_00_00) >> 26 {
                0b0 => MultichannelType::StandardLoudspeakerLayout,
                _ => MultichannelType::Unknown,
            },
            channel_information: flags & 0x00_FF_FF_FF,
        },
    ))
}

fn dial_norm_adjust(raw: u8) -> i8 {
    if raw == 0 {
        -31
    } else if raw > 0 && raw < 64 {
        -(raw as i8)
    } else {
        panic!("dial_norm value above 63 is illegal; got {}", raw)
    }
}

fn mix_level_adjust(raw: u8) -> u8 {
    if raw < 64 {
        raw + 70
    } else {
        panic!("mix_level value above 63 is illegal; got {}", raw)
    }
}

fn channel_meaning(input: &[u8]) -> IResult<&[u8], ChannelMeaning> {
    let (rest, main_data) = be_u64(input)?;

    // x_ch_control_enabled, top 9 bytes, of which we care about the bottom 3
    let ce = (main_data >> 55) as u16;
    let ctrl_enabled = ControlEnabled {
        two_ch: (ce >> 13) != 0,
        six_ch: (ce >> 14) != 0,
        eight_ch: (ce >> 15) != 0,
    };

    // drc_start_up_gain
    let drc_start_up_gain = ((main_data >> 47) & 0x7F) as i8;

    let multi_ch_field = main_data >> 2;

    // dial_norm
    let dial_norm = DialNorm {
        two_ch: dial_norm_adjust(((multi_ch_field >> 39) & 0x3F) as u8),
        six_ch: dial_norm_adjust(((multi_ch_field >> 28) & 0x1F) as u8),
        eight_ch: dial_norm_adjust(((multi_ch_field >> 12) & 0x1F) as u8),
    };

    // mix_level
    let mix_level = MixLevel {
        two_ch: mix_level_adjust(((multi_ch_field >> 33) & 0x3F) as u8),
        six_ch: mix_level_adjust(((multi_ch_field >> 22) & 0x3F) as u8),
        eight_ch: mix_level_adjust(((multi_ch_field >> 6) & 0x3F) as u8),
    };

    // source_format
    let source_format = ChSourceFormat {
        six_ch: ((multi_ch_field >> 17) & 0x1F) as u8,
        eight_ch: (multi_ch_field & 0x3F) as u8,
    };

    let channel_meaning = ChannelMeaning {
        ctrl_enabled,
        dial_norm,
        mix_level,
        source_format,
        drc_start_up_gain,
        extra_channel_meaning: None,
    };

    let extra_present = (main_data & 0b1) != 0;
    if extra_present {
        // skip extra data for now

        let (rest, extra_dword) = be_u16(rest)?;
        // extra length is given in 16 bit, we want to know how many bytes
        let extra_length = (extra_dword >> 12) * 2;
        let expansion_length = extra_length + 2;

        // for now let's skip this extra data
        let (rest, _) = take(extra_length)(input)?;

        return Ok((rest, channel_meaning));
    }

    Ok((rest, channel_meaning))
}

fn major_sync_info(input: &[u8]) -> IResult<&[u8], MajorSyncInfo> {
    do_parse!(
        input,
        tag!(&[0xF8][..]) >>
        tag!(&[0x72][..]) >>
        tag!(&[0x6F][..]) >>
        tag!(&[0xBA][..]) >>
        format_info: format_info >>
        tag!(&[0xB7][..]) >>
        tag!(&[0x52][..]) >>
        flags: be_u16 >>
        take!(2) >> // reserved (v16)
        data_rate: be_u16 >>
        substream_field: be_u16 >>
        channel_meaning: channel_meaning >>
        crc: be_u16 >>
        (MajorSyncInfo {
            format_info,
            flags,
            variable_rate: ((data_rate & 0x80_00) >> 15) != 0,
            peak_data_rate: data_rate & 0x7F_FF,
            substreams: ((substream_field & 0xF0_00) >> 12) as u8,
            extended_substream_info: ((substream_field & 0x03_00) >> 8) as u8,
            substream_info: (substream_field & 0x00_FF) as u8,
            channel_meaning,
            crc,
        })
    )
}

fn sync_header(input: &[u8]) -> IResult<&[u8], SyncHeader> {
    // let (rest, (_, length)) = nibble_and_au_length(input)?;
    // let x = bits_tuple((
    //     nom::bits::streaming::take(16u8),
    //     nom::bits::streaming::tag(0xF8726FBA, 32usize),
    // ))(rest)?;

    let (rest, (a, b, ms)) = tuple((be_u16, be_u16, opt(major_sync_info)))(input)?;

    Ok((
        rest,
        SyncHeader {
            access_unit_length: (a & 0x0FFF) * 2,
            input_timing: b,
            major_sync_info: ms,
        },
    ))
}

#[test]
fn format_info_test() {
    let data = vec![0x00, 0x17, 0x80, 0x4F, 0xB7];
    let sl = &data[..];

    assert_eq!(
        format_info(sl),
        Ok((
            &sl[4..],
            FormatInfo {
                sampling_frequency: SamplingFrequency::k48,
                six_ch_multichannel_type: MultichannelType::StandardLoudspeakerLayout,
                eight_ch_multichannel_type: MultichannelType::StandardLoudspeakerLayout,
                channel_information: 0x17804F
            }
        ))
    );
}

#[test]
fn major_sync_info_test() {
    let data = include_bytes!("../../assets/frozen2-segment55-frame1.bin");
    let sl = &data[4..];

    dbg!(major_sync_info(sl).unwrap().1);

    // assert_eq!(
    //     major_sync_info(sl),
    //     Ok((
    //         &sl[30..],
    //         MajorSyncInfo {

    //         }
    //     ))
    // );
}

#[test]
fn sync_header_test() {
    let data = include_bytes!("../../assets/frozen2-segment55-frame1.bin");
    let sl = &data[..];

    let header = sync_header(sl);

    assert_eq!(
        header,
        Ok((
            &sl[26..],
            SyncHeader {
                access_unit_length: 768,
                input_timing: 7144,
                major_sync_info: Some(MajorSyncInfo {
                    format_info: FormatInfo {
                        sampling_frequency: SamplingFrequency::k48,
                        six_ch_multichannel_type: MultichannelType::StandardLoudspeakerLayout,
                        eight_ch_multichannel_type: MultichannelType::StandardLoudspeakerLayout,
                        channel_information: 1540175, // todo
                    },
                    flags: 0x1000,
                    variable_rate: true,
                    peak_data_rate: 3546,
                    substreams: 4,
                    extended_substream_info: 1, // todo
                    substream_info: 204,        // todo
                    channel_meaning: ChannelMeaning {
                        ctrl_enabled: ControlEnabled {
                            two_ch: false,
                            six_ch: false,
                            eight_ch: false,
                        },
                        dial_norm: DialNorm {
                            two_ch: -31,
                            six_ch: -31,
                            eight_ch: -31,
                        },
                        mix_level: MixLevel {
                            two_ch: 105,
                            six_ch: 105,
                            eight_ch: 105,
                        },
                        source_format: ChSourceFormat {
                            six_ch: 0,
                            eight_ch: 0
                        },
                        drc_start_up_gain: 0,
                        extra_channel_meaning: None, // todo
                    },
                    crc: 16159,
                }),
            }
        ))
    );

    assert_eq!(
        header
            .unwrap()
            .1
            .major_sync_info
            .unwrap()
            .peak_data_rate_bps(),
        10_638_000.0
    );
}

#[test]
fn nibble_and_au_length_test() {
    let data = vec![0x51, 0x80, 0x1B, 0xE8];
    let sl = &data[..];

    assert_eq!(nibble_and_au_length(sl), Ok((&sl[2..], (0x5, 0x180))));
}

#[test]
fn test_xor_u8() {
    let byte = 0x80u8;
    let xor = (byte ^ (byte << 4)) >> 4;
    assert_eq!(xor, 0b1000);
}
