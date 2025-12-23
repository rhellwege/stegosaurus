use anyhow::Result;
use std::io::{Read, Write};

pub mod arith;
mod bitstream;
pub mod lzss;

pub trait Compressor {
    fn compress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
    fn decompress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
}

pub trait DataTransform<'a>: Read + 'a {
    fn attach_reader(&mut self, src: Box<dyn Read + 'a>);
    fn as_reader(&'a mut self) -> Box<dyn Read + 'a> {
        Box::new(self)
    }
}

pub struct Pipeline<'a> {
    transform: Box<dyn DataTransform<'a>>,
}

impl<'a> Pipeline<'a> {
    pub fn new(transform: Box<dyn DataTransform<'a> + 'a>) -> Self {
        Pipeline { transform }
    }

    pub fn pipe(self, mut new_transform: Box<dyn DataTransform<'a> + 'a>) -> Self {
        new_transform.attach_reader(self.transform);

        Self {
            transform: new_transform,
        }
    }
}

impl<'a> Read for Pipeline<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.transform.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use crate::compression::bitstream::BitStream;

    use super::*;

    #[test]
    fn bitstream_pipeline() {
        let data_src = b"Hello, World!";
        let mut bs = BitStream::new();
        bs.attach_reader(Box::new(data_src.as_slice()));
        let mut bs2 = BitStream::new();
        let mut p = Pipeline::new(Box::new(bs)).pipe(Box::new(bs2));
        let mut output = Vec::new();
        let nread = p.read_to_end(&mut output).unwrap();
        assert_eq!(nread, data_src.len());
        assert_eq!(output, data_src);
    }
}
