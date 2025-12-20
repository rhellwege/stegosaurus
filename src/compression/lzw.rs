use super::Compressor;

pub struct LzwCompressor;

impl Compressor for LzwCompressor {
    fn deflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        // Implementation of LZW compression algorithm
        unimplemented!()
    }

    fn inflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        // Implementation of LZW decompression algorithm
        unimplemented!()
    }
}
