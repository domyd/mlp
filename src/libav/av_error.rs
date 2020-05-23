use log::error;
use std::{fmt::Display, io, path::PathBuf};

#[derive(Debug)]
pub enum AVError {
    IoErr(std::io::Error),
    FFMpegErr(i32),
    DemuxErr(DemuxErr),
    OtherErr(OtherErr),
}

#[derive(Debug)]
pub enum DemuxErr {
    NoVideoStreamFound,
    NoTrueHdStreamFound,
    NoTrueHdFramesEncountered,
}

#[derive(Debug)]
pub enum OtherErr {
    FilePathIsNotUtf8(PathBuf),
}

impl From<DemuxErr> for AVError {
    fn from(err: DemuxErr) -> Self {
        AVError::DemuxErr(err)
    }
}

impl From<OtherErr> for AVError {
    fn from(err: OtherErr) -> Self {
        AVError::OtherErr(err)
    }
}

impl From<io::Error> for AVError {
    fn from(err: io::Error) -> Self {
        AVError::IoErr(err)
    }
}

impl AVError {
    pub fn log(&self) {
        error!("{}", self);
    }
}

impl std::error::Error for AVError {}

impl Display for AVError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AVError::IoErr(e) => write!(f, "{}", e),
            AVError::FFMpegErr(i) => write!(f, "ffmpeg error code: {}", i),
            AVError::DemuxErr(e) => {
                let msg = match e {
                    DemuxErr::NoTrueHdStreamFound => "No TrueHD stream found.",
                    DemuxErr::NoVideoStreamFound => "No video stream found.",
                    DemuxErr::NoTrueHdFramesEncountered => "No TrueHD frames encountered.",
                };
                write!(f, "{}", msg)
            }
            AVError::OtherErr(e) => {
                let msg = match e {
                    OtherErr::FilePathIsNotUtf8(path) => {
                        format!("File path is not valid UTF-8: {}", path.to_string_lossy())
                    }
                };
                write!(f, "{}", msg)
            }
        }
    }
}
