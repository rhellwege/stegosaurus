use anyhow::Result;
use std::io::{Read, Write};

pub mod arith;
mod bitstream;
pub mod huffman;

pub trait Compressor {
    fn deflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
    fn inflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
}
