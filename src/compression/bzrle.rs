use std::io::Read;

use crate::compression::{DataTransform, bitstream::BitStream};

fn u32_to_bijective(mut n: u32, symbol_a: u16, symbol_b: u16) -> Vec<u16> {
    let mut result: Vec<u16> = Vec::new();
    let mut i = 0;
    while n > 0 {
        let place = 1 << i;
        if n % (place * 2) == 0 {
            result.push(symbol_b);
            n -= place * 2;
        } else {
            result.push(symbol_a);
            n -= place;
        }
        i += 1;
    }
    return result;
}

fn bijective_to_u32(bijective: &[u16], symbol_a: u16, symbol_b: u16) -> u32 {
    let mut total = 0;
    for i in 0..bijective.len() {
        let place = 1 << i;
        if bijective[i] == symbol_a {
            total += place;
        } else {
            total += place * 2;
        }
    }
    return total;
}

pub struct BzrleEncoder {
    src: Option<Box<dyn Read>>,
    zero_count: u32,
    output_bs: BitStream,
    symbol_a: u16,
    symbol_b: u16,
    bits_per_symbol: u8,
}

impl DataTransform for BzrleEncoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = Some(src);
    }
}

impl BzrleEncoder {
    pub fn new(symbol_a: u16, symbol_b: u16, bits_per_symbol: u8) -> Self {
        BzrleEncoder {
            src: None,
            zero_count: 0,
            output_bs: BitStream::new(),
            symbol_a: symbol_a,
            symbol_b: symbol_b,
            bits_per_symbol: bits_per_symbol,
        }
    }

    fn flush_count(&mut self) {
        if self.zero_count > 0 {
            let bijective = u32_to_bijective(self.zero_count, self.symbol_a, self.symbol_b);
            for sym in bijective {
                self.output_bs
                    .write_n_bits_u64(self.bits_per_symbol, sym as u64);
            }
            self.zero_count = 0;
        }
    }
}

impl Read for BzrleEncoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_bits = buf.len() * 8;
        let mut byte_buf = [0u8; 1];
        let mut src = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        while self.output_bs.bits_in_stream() < requested_bits {
            let nread = src.read(&mut byte_buf)?;
            if nread == 0 {
                self.flush_count();
                break;
            }

            if byte_buf[0] == 0 {
                self.zero_count += 1;
                continue;
            }

            self.flush_count();
            self.output_bs
                .write_n_bits_u64(self.bits_per_symbol, byte_buf[0] as u64);
        }

        self.src = Some(src);
        self.output_bs.read(buf)
    }
}

pub struct BzrleDecoder {
    src: Option<BitStream>,
    output_bs: BitStream,
    symbol_a: u16,
    symbol_b: u16,
    bits_per_symbol: u8,
}

impl DataTransform for BzrleDecoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        let mut bs = BitStream::new();
        bs.attach_reader(src);
        self.src = Some(bs);
    }
}

impl BzrleDecoder {
    pub fn new(symbol_a: u16, symbol_b: u16, bits_per_symbol: u8) -> Self {
        BzrleDecoder {
            src: None,
            output_bs: BitStream::new(),
            symbol_a: symbol_a,
            symbol_b: symbol_b,
            bits_per_symbol: bits_per_symbol,
        }
    }
}

impl Read for BzrleDecoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_bits = buf.len() * 8;
        let mut sym_buf = 0u64;
        let mut bs = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        while self.output_bs.bits_in_stream() < requested_bits {
            let mut nread = bs
                .read_n_bits_u64(self.bits_per_symbol, &mut sym_buf)
                .map_err(|_| std::io::Error::other("Failed to read symbol"))?;
            if nread == 0 {
                break;
            }

            let mut bi: Vec<u16> = Vec::new();
            while nread != 0 && (sym_buf as u16 == self.symbol_a || sym_buf as u16 == self.symbol_b)
            {
                bi.push(sym_buf as u16);
                nread = bs
                    .read_n_bits_u64(self.bits_per_symbol, &mut sym_buf)
                    .map_err(|_| std::io::Error::other("Failed to read symbol"))?;
            }

            if !bi.is_empty() {
                let nzeros = bijective_to_u32(&bi, self.symbol_a, self.symbol_b);
                for _ in 0..nzeros {
                    self.output_bs.write_byte(0x0);
                }
            }

            if nread != 0 {
                self.output_bs.write_byte(sym_buf as u8);
            }
        }

        self.src = Some(bs);
        self.output_bs.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bijective_numeration() {
        let test_cases = vec![12345u32, 892057u32, 5u32, 1u32, 2u32, 73458933u32];

        for n in test_cases {
            let bi = u32_to_bijective(n, 0, 1);
            dbg!(&bi);
            let y = bijective_to_u32(&bi, 0, 1);
            assert_eq!(n, y);
        }
    }

    #[test]
    fn e2ebz() {
        let test_cases = vec![
            vec![0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1, 2, 5, 3, 0],
            vec![0, 0, 0, 2, 8, 0, 0, 1, 2, 5, 3, 6],
            vec![2, 8, 0, 0, 1, 2, 5, 3, 6, 0, 0, 0],
        ];
        let mut encoder = BzrleEncoder::new(0, 256, 16);
        let mut decoder = BzrleDecoder::new(0, 256, 16);

        for test in test_cases {
            dbg!(&test);
            let mut encoded: Vec<u8> = Vec::new();
            encoder.attach_reader(Box::new(std::io::Cursor::new(test.clone())));
            encoder.read_to_end(&mut encoded);
            dbg!(&encoded);
            decoder.attach_reader(Box::new(std::io::Cursor::new(encoded.clone())));
            let mut output: Vec<u8> = Vec::new();
            decoder.read_to_end(&mut output);
            assert!(output == test);
        }
    }
}
