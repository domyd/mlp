pub mod mlp_frame_reader;
pub mod mlp_iterator;
//pub mod mlp_parser;

pub use mlp_frame_reader::MlpFrameReader;
pub use mlp_iterator::MlpIterator;

pub struct MlpFrame {
    pub segment: u16,
    pub offset: usize,
    pub length: usize,
    pub input_timing: u16,
    pub has_major_sync: bool,
}
