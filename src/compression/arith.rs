use super::Compressor;
use super::bitstream::BitStream;
use anyhow::{Context, Result, anyhow};
use num::{BigRational, traits::SaturatingMul};
use std::io::{Read, Write};

const ONE: u64 = 0xffffffffffffff;
const ONE_HALF: u64 = ONE >> 1;
const ONE_FOURTH: u64 = ONE_HALF >> 1;
const THREE_FOURTHS: u64 = ONE_FOURTH * 3;
const EOF_SYMBOL: u16 = 256;

#[derive(Debug)]
pub struct ProbabilityInterval {
    lower: u64,
    upper: u64,
    denom: u64,
}

pub trait Model {
    fn get_probability(&mut self, symbol: u16) -> ProbabilityInterval;
    fn get_symbol(&mut self, scaled_value: u64) -> Option<u8>; // if none, EOF
}

pub struct AdaptiveModel {
    cum_freqs: [u32; 258],
}

impl AdaptiveModel {
    pub fn new() -> Self {
        let mut cum_freqs: [u32; 258] = [0; 258];
        for i in 1..258 {
            cum_freqs[i] = i as u32;
        }
        AdaptiveModel {
            cum_freqs: cum_freqs,
        }
    }

    /// must be called every time we read a symbol
    pub fn update_freq(&mut self, symbol: u8) {
        for i in (symbol as usize + 1)..258 {
            self.cum_freqs[i] += 1;
        }
    }

    pub fn count(&mut self) -> u32 {
        return self.cum_freqs[257];
    }
}

impl Model for AdaptiveModel {
    /// Automatically updates the model
    fn get_probability(&mut self, symbol: u16) -> ProbabilityInterval {
        let p = ProbabilityInterval {
            lower: self.cum_freqs[symbol as usize] as u64,
            upper: self.cum_freqs[symbol as usize + 1] as u64,
            denom: self.count() as u64,
        };

        self.update_freq(symbol as u8);

        return p;
    }

    fn get_symbol(&mut self, scaled_value: u64) -> Option<u8> {
        None
    }
}

pub struct ArithmeticCompressor {
    model: Box<dyn Model>,
    bitstream: BitStream,
    pending_bits: usize,
}

impl ArithmeticCompressor {
    pub fn new_adaptive() -> Self {
        ArithmeticCompressor {
            model: Box::new(AdaptiveModel::new()),
            bitstream: BitStream::new(),
            pending_bits: 0,
        }
    }

    // https://web.archive.org/web/20241113133144/https://marknelson.us/posts/2014/10/19/data-compression-with-arithmetic-coding.html
    fn write_bits_for_symbol(&mut self, symbol: u16, high: &mut u64, low: &mut u64) {
        dbg!(symbol, &high, &low);
        let range = *high - *low + 1;

        let p = self.model.get_probability(symbol);
        dbg!(&p);

        *high = *low + range * p.upper / p.denom - 1;
        *low = *low + range * p.lower / p.denom;
        dbg!(symbol, &high, &low);

        loop {
            if *high < ONE_HALF {
                self.output_bit_plus_pending(false);
            } else if *low >= ONE_HALF {
                self.output_bit_plus_pending(true);
            } else if *low >= ONE_FOURTH && *high < THREE_FOURTHS {
                self.pending_bits += 1;
                *low -= ONE_FOURTH;
                *high -= ONE_FOURTH;
            } else {
                break;
            }
            *high <<= 1;
            *high += 1;
            *low <<= 1;
            *high &= ONE;
            *low &= ONE;
        }
    }

    pub fn encode(&mut self, source: impl Read) {
        println!("Encode");
        let mut high = ONE;
        let mut low = 0;

        for next in source.bytes() {
            if let Ok(symbol) = next {
                self.write_bits_for_symbol(symbol as u16, &mut high, &mut low);
            }
        }

        // write eof bits
        self.write_bits_for_symbol(EOF_SYMBOL, &mut high, &mut low);
    }

    pub fn decode() {}

    fn output_bit_plus_pending(&mut self, bit: bool) {
        self.bitstream.write_bit(bit);

        while self.pending_bits > 0 {
            self.bitstream.write_bit(!bit);
            self.pending_bits -= 1;
        }
    }
}

impl Compressor for ArithmeticCompressor {
    fn compress(&mut self, input_stream: impl Read, mut output_stream: impl Write) -> Result<()> {
        println!("Compress");
        self.encode(input_stream);
        println!("Encoded!");
        let mut bytes = Vec::new();
        let _ = self.bitstream.read_to_end(&mut bytes)?;
        output_stream.write_all(&bytes)?;
        Ok(())
    }

    fn decompress(&mut self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compress() {
        let in_bytes = b"abcghabcdefghijabcdefghklmnopqrstuvwxyz";
        let mut compressor = ArithmeticCompressor::new_adaptive();

        let mut out_bytes = Vec::new();
        let _ = compressor.compress(in_bytes.as_slice(), &mut out_bytes);
        dbg!(&out_bytes, out_bytes.len());
    }
}
