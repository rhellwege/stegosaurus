use anyhow::Result;
use std::io::{Read, Write};

pub mod arith;
mod bitstream;
pub mod lzss;

pub trait Compressor {
    fn compress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
    fn decompress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
}

pub trait DataTransform: Read {
    fn from_reader(src: Box<dyn Read>) -> Self;
}
