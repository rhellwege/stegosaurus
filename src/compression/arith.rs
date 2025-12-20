use super::Compressor;
use super::bitstream::BitStream;
use anyhow::{Context, Result, anyhow};
use num::BigRational;
use std::io::{Read, Write};

const ONE_FOURTH: u32 = 0x40000000;
const ONE_HALF: u32 = 0x80000000;
const THREE_FOURTHS: u32 = 0xC0000000;
const ONE: u32 = 0xFFFFFFFF;

pub struct FrequencyTable {
    frequencies: Vec<u32>,
}

pub struct ProbabilityInterval {
    lower: u32,
    upper: u32,
    denom: u32,
}

pub trait Model {
    fn get_probability(&mut self, symbol: u8) -> ProbabilityInterval;
    fn get_symbol(&mut self, value: u32) -> u8;
}

pub struct ArithmeticCompressor {
    model: Box<dyn Model>,
}

impl ArithmeticCompressor {
    pub fn new() -> Self {
        ArithmeticCompressor {}
    }
}

impl Compressor for ArithmeticCompressor {
    fn deflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        todo!()
    }

    fn inflate(&self, input_stream: impl Read, output_stream: impl Write) -> Result<()> {
        todo!()
    }
}
