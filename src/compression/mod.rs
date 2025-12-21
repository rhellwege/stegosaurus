use anyhow::Result;
use std::io::{Read, Write};

pub mod arith;
mod bitstream;
pub mod huffman;

pub trait Compressor {
    fn compress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
    fn decompress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
}
