use anyhow::Result;
use std::io::{Read, Write};

pub mod arith;
mod bitstream;
pub mod bwt;
pub mod lzss;
pub mod mtf;
pub mod rle;

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
    transform: Option<Box<dyn DataTransform>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline { transform: None }
    }

    pub fn from_reader(reader: Box<dyn Read>) -> Self {
        Pipeline {
            transform: Some(Box::new(IdentityTransform { src: reader })),
        }
    }

    pub fn pipe(self, mut new_transform: Box<dyn DataTransform>) -> Self {
        if let Some(prev_transform) = self.transform {
            new_transform.attach_reader(prev_transform);
        }

        Self {
            transform: Some(new_transform),
        }
    }
}

impl DataTransform for Pipeline {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.transform = Some(Box::new(IdentityTransform { src }));
    }
}

impl Read for Pipeline {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(ref mut transform) = self.transform {
            transform.read(buf)
        } else {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::compression::{
        arith::{AriDecoder, AriEncoder},
        bitstream::BitStream,
        mtf::{MtfDecoder, MtfEncoder},
    };

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

    #[test]
    fn ari_mtf_pipeline() {
        let src = b"ooijiorgiojio3jioij3jkjn3ngvj3jk49042fjpqjpqr[0r9 q0n[q 0r qer,. ew ,..,er., mwe mmwerl nwe;r nejr nkjkjkafieifijioi34g3gr[g[g[[er[g[e[[[gpwigij3ogookbn  e bkjjkerwjkkll3go4poop3poppv3op4mv34ompv3popom34o3vop34mosfjkglfdlkmm;sdfljgnjksnjktjnkrgjknrtkjnjnkrjjnjkbjknnjjjnnjjnjnjnnjnjbgnbgnbgnngbngbngbngbnngbngbnbgnngbngbnnbgngbnngb]]]]]]]]]]]";
        let mut encoder = Pipeline::from_reader(Box::new(std::io::Cursor::new(src)))
            .pipe(Box::new(MtfEncoder::new()))
            .pipe(Box::new(AriEncoder::new_adaptive_bytes()));

        let mut output_bytes = Vec::new();
        let _ = encoder.read_to_end(&mut output_bytes);
        println!("{} ==> {}", src.len(), output_bytes.len());
        assert!(output_bytes.len() > 0);

        let mut decoder = Pipeline::from_reader(Box::new(std::io::Cursor::new(output_bytes)))
            .pipe(Box::new(AriDecoder::new_adaptive_bytes()))
            .pipe(Box::new(MtfDecoder::new()));

        let mut copy_bytes = Vec::new();
        let _ = decoder.read_to_end(&mut copy_bytes);
        assert_eq!(src, copy_bytes.as_slice());
    }
}
