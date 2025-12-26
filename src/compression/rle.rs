use std::io::{Read, Write};

use crate::compression::DataTransform;

pub const RUN_THRESHOLD: u8 = 4;
pub const MAX_REPEAT: u8 = u8::MAX;

pub struct RleEncoder {
    src: Option<Box<dyn Read>>,
    most_recent: u8,
    run: u16,
    buf: Option<u8>,
}

impl RleEncoder {
    pub fn new() -> Self {
        Self {
            src: None,
            most_recent: 0,
            run: 0,
            buf: None,
        }
    }

    // None means eof
    pub fn next_byte(&mut self) -> Option<u8> {
        let mut r = match self.src.take() {
            Some(s) => s,
            None => return None,
        };

        loop {
            if let Some(leftover) = self.buf {
                self.buf = None;
                self.src = Some(r);
                return Some(leftover);
            }
            let mut byte = [0u8; 1];

            let nread = r.read(&mut byte).ok()?;
            if nread == 0 {
                if self.run > RUN_THRESHOLD as u16 {
                    self.src = Some(r);
                    let ret = Some((self.run - RUN_THRESHOLD as u16) as u8);
                    self.run = 0;
                    return ret;
                }
                self.src = Some(r);
                return None;
            }

            if byte[0] == self.most_recent && self.run != 0 {
                self.run += 1;
            } else {
                if self.run >= RUN_THRESHOLD as u16 {
                    self.buf = Some(byte[0]);
                    self.src = Some(r);
                    let ret = Some((self.run - RUN_THRESHOLD as u16) as u8);
                    self.run = 1;
                    return ret;
                }
                self.run = 1;
            }

            if self.run >= MAX_REPEAT as u16 + RUN_THRESHOLD as u16 {
                self.src = Some(r);
                let ret = Some((self.run - RUN_THRESHOLD as u16) as u8);
                self.run = 1;
                return ret;
            }
            self.most_recent = byte[0];

            if self.run <= RUN_THRESHOLD as u16 {
                self.src = Some(r);
                return Some(byte[0]);
            }
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
        let mut i = 0;

        while let Some(byte) = self.next_byte() {
            buf[i] = byte;
            i += 1;

            if i >= buf.len() {
                return Ok(i);
            }
        }

        return Ok(i);
    }
}

pub struct RleDecoder {
    src: Option<Box<dyn Read>>,
    input_run: u8,
    output_run: u8,
    most_recent: u8,
    check_rl: bool,
}

impl RleDecoder {
    pub fn new() -> Self {
        RleDecoder {
            src: None,
            input_run: 0,
            output_run: 0,
            most_recent: 0,
            check_rl: false,
        }
    }

    /// return of None means EOF
    pub fn next_byte(&mut self) -> Option<u8> {
        let mut r = match self.src.take() {
            Some(s) => s,
            None => return None,
        };

        if self.output_run > 0 {
            self.output_run -= 1;
            self.src = Some(r);
            return Some(self.most_recent);
        }

        let mut byte = [0u8; 1];

        let nread = r.read(&mut byte).ok()?;
        if nread == 0 {
            self.src = Some(r);
            return None;
        }

        if self.most_recent == byte[0] {
            self.input_run += 1;
        } else {
            self.input_run = 1;
        }

        if self.check_rl {
            self.check_rl = false;
            self.output_run = byte[0];
            // meaningless run length, continue with next byte
            if self.output_run == 0 {
                let nread = r.read(&mut byte).ok()?;
                if nread != 0 {
                    self.most_recent = byte[0];
                }
            } else {
                self.output_run -= 1;
            }
            self.src = Some(r);
            return Some(self.most_recent);
        }

        if self.input_run == RUN_THRESHOLD {
            self.check_rl = true;
        }

        if self.input_run == RUN_THRESHOLD {
            self.input_run += 1; // trigger the output run next
        }

        self.most_recent = byte[0];
        self.src = Some(r);
        return Some(self.most_recent);
    }
}

impl DataTransform for RleDecoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = Some(src);
    }
}

impl Read for RleDecoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut i = 0;

        while let Some(byte) = self.next_byte() {
            buf[i] = byte;
            i += 1;

            if i >= buf.len() {
                return Ok(i);
            }
        }

        return Ok(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static test_cases: &[(&[u8], &[u8])] = &[
        (&[0x0, 0x0, 0x0, 0x0, 0x0], &[0x0, 0x0, 0x0, 0x0, 0x1]),
        (&[0u8; 261], &[0x0, 0x0, 0x0, 0x0, 0xff, 0x0, 0x0]),
        (&[255u8; 260], &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
        (&[1u8; 261], &[0x1, 0x1, 0x1, 0x1, 0xff, 0x1, 0x1]),
        (&[0x1, 0x1, 0x1, 0x1, 0x1], &[0x1, 0x1, 0x1, 0x1, 0x1]),
        // (&[0x0, 0x0, 0x0, 0x0], &[0x0, 0x0, 0x0, 0x0, 0x0]),
        (&[0x0, 0x0, 0x0, 0x0, 0x0, 0x0], &[0x0, 0x0, 0x0, 0x0, 0x2]),
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

    #[test]
    fn encode() {
        for (input, expected) in test_cases.iter() {
            let mut encoder = RleEncoder::new();
            encoder.attach_reader(Box::new(std::io::Cursor::new(input.clone())));

            dbg!(input);

            let mut output = Vec::new();
            encoder.read_to_end(&mut output);
            assert_eq!(**expected, *output);
        }
    }

    #[test]
    fn decode() {
        for (expected, input) in test_cases.iter() {
            let mut decoder = RleDecoder::new();
            decoder.attach_reader(Box::new(std::io::Cursor::new(input.clone())));

            dbg!(input);

            let mut output = Vec::new();
            decoder.read_to_end(&mut output);
            assert_eq!(**expected, *output);
        }
    }
}
