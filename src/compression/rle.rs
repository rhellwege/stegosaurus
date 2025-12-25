use std::io::Read;

use crate::compression::DataTransform;

pub const REPEAT_THRESHOLD: u8 = 4;
pub const MAX_REPEAT: u8 = u8::MAX;

pub struct RleEncoder {
    src: Option<Box<dyn Read>>,
    most_recent: u8,
    run: u16,
}

impl RleEncoder {
    pub fn new() -> Self {
        Self {
            src: None,
            most_recent: 0,
            run: 0,
        }
    }
}

impl DataTransform for RleEncoder {
    fn attach_reader(&mut self, reader: Box<dyn Read>) {
        self.src = Some(reader);
    }
}

impl Read for RleEncoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {}
    }
}

pub struct RleDecoder {
    src: Box<dyn Read>,
    window: Vec<u8>,
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn encode() {
        let test_cases: Vec<(&[u8], &[u8])> = vec![
            (&[0x1, 0x2, 0x2, 0x3, 0x4], &[0x1, 0x2, 0x2, 0x3, 0x4]),
            (&[0x1, 0x1, 0x1, 0x1, 0x2], &[0x1, 0x1, 0x1, 0x1, 0x0, 0x2]),
            (&[0x1, 0x1, 0x1, 0x1, 0x1], &[0x1, 0x1, 0x1, 0x1, 0x1]),
            (&[0x1, 0x1, 0x1, 0x1, 0x1, 0x1], &[0x1, 0x1, 0x1, 0x1, 0x2]),
            (&[0x2, 0x2, 0x2, 0x2, 0x2, 0x2], &[0x2, 0x2, 0x2, 0x2, 0x2]),
            (
                &[0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x0],
                &[0x2, 0x2, 0x2, 0x2, 0x6, 0x0],
            ),
            (
                &[0xf, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x0],
                &[0xf, 0x2, 0x2, 0x2, 0x2, 0x5, 0x0],
            ),
        ];

        for (input, expected) in test_cases.into_iter() {
            let mut encoder = RleEncoder::new();
            encoder.attach_reader(Box::new(std::io::Cursor::new(input)));

            let mut output = Vec::new();
            encoder.read_to_end(&mut output);
            assert_eq!(output.as_slice(), expected);
        }
    }
}
