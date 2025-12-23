use super::DataTransform;
use std::io::Read;
// Move to front transform

pub struct MtfEncoder {
    src: Option<Box<dyn Read>>,
    mapping: [u8; 256],
}

impl MtfEncoder {
    pub fn new() -> Self {
        let mut mapping: [u8; 256] = [0; 256];
        for i in 0..256 {
            mapping[i] = i as u8;
        }
        MtfEncoder {
            src: None,
            mapping: mapping,
        }
    }

    /// puts the element at index index into 0 and shifts everything to the right.
    fn shift_index(&mut self, index: u8) {
        let value = self.mapping[index as usize];
        let mut i = index as usize;
        while i > 0 {
            self.mapping[i] = self.mapping[i - 1];
            i -= 1;
        }
        self.mapping[0] = value;
    }

    fn encode_byte(&mut self, byte: u8) -> u8 {
        let rank = self.mapping.iter().position(|&b| b == byte).unwrap() as u8;
        self.shift_index(rank);
        rank
    }
}

impl DataTransform for MtfEncoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = Some(src);
    }
}

impl Read for MtfEncoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut src_reader = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        let mut i = 0;
        while i < buf.len() {
            let mut byte = [0u8; 1];
            if let Ok(nread) = src_reader.read(&mut byte) {
                if nread == 0 {
                    break;
                }
                buf[i] = self.encode_byte(byte[0]);
                i += 1;
            } else {
                // Before we return the error, put the reader back.
                self.src = Some(src_reader);
                return Err(std::io::Error::other("failed to read byte"));
            }
        }

        self.src = Some(src_reader);
        Ok(i)
    }
}

pub struct MtfDecoder {
    src: Option<Box<dyn Read>>,
    mapping: [u8; 256],
}

impl MtfDecoder {
    pub fn new() -> Self {
        let mut mapping: [u8; 256] = [0; 256];
        for i in 0..256 {
            mapping[i] = i as u8;
        }
        MtfDecoder {
            src: None,
            mapping: mapping,
        }
    }

    /// puts the element at index index into 0 and shifts everything to the right.
    fn shift_index(&mut self, index: u8) {
        let value = self.mapping[index as usize];
        let mut i = index as usize;
        while i > 0 {
            self.mapping[i] = self.mapping[i - 1];
            i -= 1;
        }
        self.mapping[0] = value;
    }

    fn decode_byte(&mut self, byte: u8) -> u8 {
        let value = self.mapping[byte as usize];
        self.shift_index(byte);
        value
    }
}

impl DataTransform for MtfDecoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = Some(src);
    }
}

impl Read for MtfDecoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut src_reader = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        let mut i = 0;
        while i < buf.len() {
            let mut byte = [0u8; 1];
            if let Ok(nread) = src_reader.read(&mut byte) {
                if nread == 0 {
                    break;
                }
                buf[i] = self.decode_byte(byte[0]);
                i += 1;
            } else {
                self.src = Some(src_reader);
                return Err(std::io::Error::other("failed to read byte"));
            }
        }

        self.src = Some(src_reader);
        Ok(i)
    }
}

#[cfg(test)]
mod tests {
    use crate::compression::Pipeline;

    use super::*;
    #[test]
    fn encoder_decoder() {
        let input = b"Hello, Wospoifjwpiofjpqijfpo3ifpoi3jfrld!";
        let e = MtfEncoder::new();
        let d = MtfDecoder::new();
        let mut p = Pipeline::from_reader(Box::new(input.as_slice()))
            .pipe(Box::new(e))
            .pipe(Box::new(d));

        let mut output = Vec::new();
        p.read_to_end(&mut output);
        assert_eq!(output, input);
    }
}
