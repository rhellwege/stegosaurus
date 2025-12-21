use super::Compressor;
use super::bitstream::BitStream;
use anyhow::{Context, Result, anyhow};
use num::BigRational;
use std::io::{Read, Write};

const ONE_FOURTH: u64 = 0x4000000000000000;
const ONE_HALF: u64 = 0x8000000000000000;
const THREE_FOURTHS: u64 = 0xC000000000000000;
const ONE: u64 = 0xFFFFFFFFFFFFFFFF;

pub struct FrequencyTable {
    frequencies: Vec<u32>,
}

pub struct ProbabilityInterval {
    lower: u64,
    upper: u64,
    denom: u64,
}

pub trait Model {
    fn get_probability(&mut self, symbol: u8) -> ProbabilityInterval;
    fn get_symbol(&mut self, scaled_value: u64) -> Option<u8>; // if none, EOF
}

pub struct AdaptiveModel {
    freqs: [u32; 257],
    cum_freqs: [u32; 257],
}

impl AdaptiveModel {
    pub fn new() -> Self {
        let freqs: [u32; 257] = [1; 257];
        let mut cum_freqs: [u32; 257] = [0; 257];
        for i in 1..257 {
            cum_freqs[i] = i as u32;
        }
        AdaptiveModel {
            freqs: freqs,
            cum_freqs: cum_freqs,
        }
    }

    /// must be called every time we read a symbol
    pub fn update_freq(&mut self, symbol: u8) {
        self.freqs[symbol as usize] += 1;
        for i in (symbol as usize + 1)..257 {
            self.cum_freqs[i] += 1;
        }
    }
}

impl Model for AdaptiveModel {
    fn get_probability(&mut self, symbol: u8) -> ProbabilityInterval {
        ProbabilityInterval {
            lower: self.cum_freqs[symbol as usize] as u64,
            upper: self.cum_freqs[symbol as usize + 1] as u64,
            denom: self.cum_freqs[256] as u64,
        }
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

    pub fn encode() {}
    pub fn decode() {}
}

impl Compressor for ArithmeticCompressor {
    fn deflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        todo!()
    }

    fn inflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        todo!()
    }
}
