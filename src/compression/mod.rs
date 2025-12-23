use anyhow::Result;
use std::io::{Read, Write};

pub mod arith;
mod bitstream;
pub mod lzss;
pub mod mtf;

pub trait Compressor {
    fn compress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
    fn decompress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()>;
}

pub trait DataTransform: Read {
    fn attach_reader(&mut self, src: Box<dyn Read>);
}

pub struct IdentityTransform {
    src: Box<dyn Read>,
}

impl DataTransform for IdentityTransform {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = src;
    }
}

impl Read for IdentityTransform {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.src.read(buf)
    }
}

pub struct Pipeline {
    transform: Box<dyn DataTransform>,
}

impl Pipeline {
    pub fn new(transform: Box<dyn DataTransform>) -> Self {
        Pipeline { transform }
    }

    pub fn from_reader(reader: Box<dyn Read>) -> Self {
        Pipeline {
            transform: Box::new(IdentityTransform { src: reader }),
        }
    }

    pub fn pipe(self, mut new_transform: Box<dyn DataTransform>) -> Self {
        new_transform.attach_reader(self.transform);

        Self {
            transform: new_transform,
        }
    }
}

impl Read for Pipeline {
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
        let mut bs2 = BitStream::new();

        let mut p = Pipeline::from_reader(Box::new(data_src.as_slice()))
            .pipe(Box::new(bs))
            .pipe(Box::new(bs2));

        let mut output = Vec::new();
        let nread = p.read_to_end(&mut output).unwrap();
        assert_eq!(nread, data_src.len());
        assert_eq!(output, data_src);
    }
}
