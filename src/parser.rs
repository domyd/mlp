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
    peak_data_rate: u16,
    substreams: u8,
    extended_substream_info: u8,
    substream_info: u8,
    crc: u16,
}

#[derive(Debug, PartialEq)]
struct FormatInfo {
    sampling_frequency: SamplingFrequency,
    _6ch_multichannel_type: MultichannelType,
    _8ch_multichannel_type: MultichannelType,
    // todo: split up more
    channel_information: u32,
}

#[derive(Debug, PartialEq)]
enum MultichannelType {
    StandardLoudspeakerLayout,
    Unknown,
}

#[derive(Debug, PartialEq)]
enum SamplingFrequency {
    _48kHz,
    _96kHz,
    _192kHz,
    _44_1kHz,
    _88_2kHz,
    _176_4kHz,
    Unknown,
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

struct ChannelMeaning {
    _2ch_control_enabled: bool,
    _6ch_control_enabled: bool,
    _8ch_control_enabled: bool,
    drc_start_up_gain: i8,
    // ranges between -1 LKFS and -63 LKFS
    _2ch_dialog_norm: i8,
    // ranges between 70 dB and 133 dB
    _2ch_mix_level: u16,
    // ranges between -1 LKFS and -31 LKFS
    _6ch_dialog_norm: i8,
    // ranges between 70 dB and 133 dB
    _6ch_mix_level: u16,
    // not sure what this is, default value is 0x00000b
    _6ch_source_format: u8,
    // ranges between -1 LKFS and -31 LKFS
    _8ch_dialog_norm: i8,
    // ranges between 70 dB and 133 dB
    _8ch_mix_level: u16,
    // not sure what this is, default value is 0x00000b
    _8ch_source_format: u8,
    extra_channel_meaning: Option<ExtraChannelMeaning>,
}

struct ExtraChannelMeaning {
    length: u8,
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
                0b0000 => SamplingFrequency::_48kHz,
                0b0001 => SamplingFrequency::_96kHz,
                0b0010 => SamplingFrequency::_192kHz,
                0b1000 => SamplingFrequency::_44_1kHz,
                0b1001 => SamplingFrequency::_88_2kHz,
                0b1010 => SamplingFrequency::_176_4kHz,
                _ => SamplingFrequency::Unknown,
            },
            _6ch_multichannel_type: match (flags & 0x08_00_00_00) >> 27 {
                0b0 => MultichannelType::StandardLoudspeakerLayout,
                _ => MultichannelType::Unknown,
            },
            _8ch_multichannel_type: match (flags & 0x04_00_00_00) >> 26 {
                0b0 => MultichannelType::StandardLoudspeakerLayout,
                _ => MultichannelType::Unknown,
            },
            channel_information: flags & 0x00_FF_FF_FF,
        },
    ))
}

fn channel_meaning(input: &[u8]) -> IResult<&[u8], u64> {
    let (rest, main_data) = be_u64(input)?;
    let extra_present = (main_data >> 63) != 0;
    if !extra_present {
        return Ok((rest, main_data));
    }

    let (rest, extra_dword) = be_u16(rest)?;
    // extra length is given in 16 bit, we want to know how many bytes
    let extra_length = (extra_dword >> 12) * 2;
    let expansion_length = extra_length + 2;

    // for now let's skip this extra data
    let (rest, _) = take(extra_length)(input)?;
    Ok((rest, main_data))
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
        channel_meaning >>
        crc: be_u16 >>
        (MajorSyncInfo {
            format_info,
            flags,
            variable_rate: ((data_rate & 0x80_00) >> 15) != 0,
            peak_data_rate: data_rate & 0x7F_FF,
            substreams: ((substream_field & 0xF0_00) >> 12) as u8,
            extended_substream_info: ((substream_field & 0x03_00) >> 8) as u8,
            substream_info: (substream_field & 0x00_FF) as u8,
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
                sampling_frequency: SamplingFrequency::_48kHz,
                _6ch_multichannel_type: MultichannelType::StandardLoudspeakerLayout,
                _8ch_multichannel_type: MultichannelType::StandardLoudspeakerLayout,
                channel_information: 0x17804F
            }
        ))
    );
}

#[test]
fn major_sync_info_test() {
    let data = include_bytes!("../assets/frozen2-segment55-accessunit1.bin");
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
    let data = include_bytes!("../assets/frozen2-segment55-accessunit1.bin");
    //let data = vec![0x51, 0x80, 0x1B, 0xE8, 0xF8, 0x72, 0x6F, 0xBA, 0x00];
    let sl = &data[..];

    assert_eq!(
        sync_header(sl),
        Ok((
            &sl[8..],
            SyncHeader {
                access_unit_length: 768,
                input_timing: 7144,
                major_sync_info: None
            }
        ))
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
