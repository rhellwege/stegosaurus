use anyhow::Result;
use std::io::{Read, Write};

use super::Compressor;

pub struct LzssCompressor;

impl Compressor for LzssCompressor {
    fn compress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        unimplemented!()
    }

    fn decompress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        unimplemented!()
    }
}
