use crate::compression::{BitStreamDataTransform, DataTransform};

use super::bitstream::BitStream;
use anyhow::{Context, Result, anyhow};
use std::io::{Read, Write};

const ONE: u64 = 0xffffffffffff;
const CODE_BITS: usize = ONE.count_ones() as usize;
const MAX_FREQ: usize = u64::MAX as usize / ONE as usize;

const ONE_HALF: u64 = (ONE >> 1) + 1;
const ONE_FOURTH: u64 = ONE_HALF >> 1;
const THREE_FOURTHS: u64 = ONE_FOURTH * 3;
const EOF_SYMBOL: u16 = 256;

#[derive(Debug)]
pub struct ProbabilityInterval {
    lower: u64,
    upper: u64,
    denom: u64,
}

pub struct AdaptiveModel {
    cum_freqs: Vec<u32>,
    max_input_symbol: u16,
    max_freq: u32,
}

impl AdaptiveModel {
    pub fn new(max_input_symbol: u16, max_freq: u32) -> Self {
        let mut am = AdaptiveModel {
            cum_freqs: vec![0; max_input_symbol as usize + 3], // 1 extra for eof 1 extra for cumulation, 1 extra for 0 index
            max_input_symbol: max_input_symbol,
            max_freq: max_freq,
        };
        am.clear();
        am
    }

    /// must be called every time we read a symbol
    pub fn update_freq(&mut self, symbol: u16) {
        if self.count() >= self.max_freq {
            self.clear();
        }
        for i in (symbol as usize + 1)..self.cum_freqs.len() {
            self.cum_freqs[i] += 1;
        }
    }

    pub fn count(&mut self) -> u32 {
        *self.cum_freqs.last().unwrap()
    }

    pub fn clear(&mut self) {
        for i in 0..self.cum_freqs.len() {
            self.cum_freqs[i] = i as u32;
        }
    }

    /// Automatically updates the model
    fn get_probability(&mut self, symbol: u16) -> ProbabilityInterval {
        let p = ProbabilityInterval {
            lower: self.cum_freqs[symbol as usize] as u64,
            upper: self.cum_freqs[symbol as usize + 1] as u64,
            denom: self.count() as u64,
        };

        self.update_freq(symbol);

        return p;
    }

    fn get_symbol(&mut self, scaled_value: u64, symbol: &mut u16) -> Option<ProbabilityInterval> {
        for i in 0u16..(self.cum_freqs.len() as u16 - 1) {
            if scaled_value < self.cum_freqs[(i + 1) as usize] as u64 {
                *symbol = i;
                return Some(self.get_probability(i));
            }
        }
        None
    }
}

pub struct AriEncoder {
    src: Option<BitStream>,
    model: AdaptiveModel,
    bits_per_symbol: u8,
    max_symbol: u16,
    output_bs: BitStream,
    pending_bits: usize,
    done: bool,
    high: u64,
    low: u64,
}

impl DataTransform for AriEncoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        let mut bs = BitStream::new();
        bs.attach_reader(src);
        self.src = Some(bs);
    }
}

impl BitStreamDataTransform for AriEncoder {
    fn output_bitstream(&mut self) -> &mut BitStream {
        &mut self.output_bs
    }
}

impl AriEncoder {
    /// the source is treated as a symbol stream. Symbols can be of any bit length. For normal bytes, use new_adaptive_bytes
    pub fn new_adaptive(bits_per_symbol: u8, max_symbol: u16) -> Self {
        assert!(
            (max_symbol as usize) < (1 << bits_per_symbol) as usize,
            "There must be room for at least one more symbol in the symbol space for the internal EOF"
        );
        AriEncoder {
            src: None,
            model: AdaptiveModel::new(max_symbol, MAX_FREQ as u32),
            bits_per_symbol: bits_per_symbol,
            max_symbol: max_symbol,
            output_bs: BitStream::new(),
            pending_bits: 0,
            done: false,
            high: ONE,
            low: 0,
        }
    }

    pub fn new_adaptive_bytes() -> Self {
        Self::new_adaptive(8, u8::MAX as u16)
    }

    fn eof_symbol(&self) -> u16 {
        self.max_symbol + 1
    }

    // https://web.archive.org/web/20241113133144/https://marknelson.us/posts/2014/10/19/data-compression-with-arithmetic-coding.html
    /// returns the number of bits written for this symbol
    fn write_bits_for_symbol(&mut self, symbol: u16) -> usize {
        let mut written = 0;
        let p = self.model.get_probability(symbol);

        let range = self.high - self.low + 1;
        self.high = self.low + range * p.upper / p.denom - 1;
        self.low = self.low + range * p.lower / p.denom;

        loop {
            if self.high < ONE_HALF {
                written += self.output_bit_plus_pending(false);
            } else if self.low >= ONE_HALF {
                written += self.output_bit_plus_pending(true);
            } else if self.low >= ONE_FOURTH && self.high < THREE_FOURTHS {
                self.pending_bits += 1;
                self.low -= ONE_FOURTH;
                self.high -= ONE_FOURTH;
            } else {
                break;
            }
            self.high <<= 1;
            self.high += 1;
            self.low <<= 1;
            self.high &= ONE;
            self.low &= ONE;
        }

        written
    }

    /// This function can only be called once at the end of the stream.
    /// It must be called to terminate the stream.
    pub fn write_eof(&mut self) -> usize {
        let mut written = 0;
        if self.done {
            return written;
        }
        written += self.write_bits_for_symbol(self.eof_symbol());
        // write outstanding bits
        self.pending_bits += 1;
        if self.low < ONE_FOURTH {
            written += self.output_bit_plus_pending(false);
        } else {
            written += self.output_bit_plus_pending(true);
        }

        self.done = true;

        written
    }

    fn output_bit_plus_pending(&mut self, bit: bool) -> usize {
        let written = 1 + self.pending_bits;
        self.output_bs.write_bit(bit);

        while self.pending_bits > 0 {
            self.output_bs.write_bit(!bit);
            self.pending_bits -= 1;
        }

        written
    }

    pub fn clear(&mut self) {
        self.output_bs.clear();
        self.model.clear();
        self.pending_bits = 0;
    }
}

impl Read for AriEncoder {
    /// Input stream must strictly be a symbol stream ie. the input length must be a multiple of the bits per symbol
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_bits = buf.len() * 8;
        let mut src_reader = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        // for each symbol in the source, output bits until there are at least as many requested bits in the output bitstream
        let mut symbol_buf = 0u64;
        loop {
            // if there are enough bits in the output stream, we are ready to read
            if self.output_bs.bits_in_stream() >= requested_bits {
                self.src = Some(src_reader);
                return self.output_bs.read(buf);
            }

            // we need more bits
            let nread = src_reader
                .read_n_bits_u64(self.bits_per_symbol, &mut symbol_buf)
                .map_err(|_| std::io::Error::other("failed to read from symbol stream"))?;

            // if there are left over bits, throw them away, our source must be byte aligned.
            // We are done, there are no more bits to encode except for eof
            if nread < self.bits_per_symbol as usize {
                let _ = self.write_eof();
                self.src = Some(src_reader);

                let mut byte_buf: u8 = 0;

                for i in 0..buf.len() {
                    let bits = self
                        .output_bs
                        .read_byte(&mut byte_buf)
                        .map_err(|_| std::io::Error::other("failed to byte from symbol stream"))?;

                    if bits == 0 {
                        return Ok(i);
                    }
                    // reading a byte puts bits into the lsb, but we want them in the msb to have trailing zeros
                    if bits < 8 {
                        byte_buf <<= 8 - bits;
                        buf[i] = byte_buf;
                        return Ok(i + 1);
                    }

                    buf[i] = byte_buf;
                }

                return Err(std::io::Error::other("failed to read for unknown reason"));
            }

            let _ = self.write_bits_for_symbol(symbol_buf as u16);
        }
    }
}

pub struct AriDecoder {
    src: Option<BitStream>,
    model: AdaptiveModel,
    bits_per_symbol: u8,
    max_symbol: u16,
    output_bs: BitStream,
    done: bool,
    high: u64,
    low: u64,
    value: u64,
}

impl DataTransform for AriDecoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        let mut bs = BitStream::new();
        bs.attach_reader(src);
        // prime the input pump
        self.value = 0;
        let bits = bs
            .read_n_bits_u64(CODE_BITS as u8, &mut self.value)
            .unwrap();
        println!("{:#064b}", self.value);
        self.value <<= CODE_BITS - bits;
        println!("{:#064b}", self.value);
        self.src = Some(bs);
    }
}

impl AriDecoder {
    /// the output is treated as a symbol stream. Symbols can be of any bit length. For normal bytes, use new_adaptive_bytes
    pub fn new_adaptive(bits_per_symbol: u8, max_symbol: u16) -> Self {
        assert!(
            (max_symbol as usize) < (1 << bits_per_symbol) as usize,
            "There must be room for at least one more symbol in the symbol space for the internal EOF"
        );
        AriDecoder {
            src: None,
            model: AdaptiveModel::new(max_symbol, MAX_FREQ as u32),
            bits_per_symbol: bits_per_symbol,
            max_symbol: max_symbol,
            output_bs: BitStream::new(),
            done: false,
            high: ONE,
            low: 0,
            value: u64::MAX, // cannot start until we have attached a reader and primed it
        }
    }

    pub fn new_adaptive_bytes() -> Self {
        Self::new_adaptive(8, u8::MAX as u16)
    }

    fn read_bits_for_symbol(&mut self) -> Option<u16> {
        if self.done {
            return None;
        }

        let range = self.high - self.low + 1;
        let scaled_value = ((self.value - self.low + 1) * self.model.count() as u64 - 1) / range;
        let mut symbol: u16 = 0;
        let p = self.model.get_symbol(scaled_value, &mut symbol)?;
        if symbol == self.eof_symbol() {
            self.done = true;
            return None;
        }

        let mut src_reader = self.src.take()?;

        // before returning the symbol we just got, renormalize high, low, value to prepare the next one
        self.high = self.low + (range * p.upper) / p.denom - 1;
        self.low = self.low + (range * p.lower) / p.denom;
        loop {
            if self.high < ONE_HALF {
                // if 0, noop
            } else if self.low >= ONE_HALF {
                self.value -= ONE_HALF; //subtract one half from all three code values
                self.low -= ONE_HALF;
                self.high -= ONE_HALF;
            } else if self.low >= ONE_FOURTH && self.high < THREE_FOURTHS {
                self.value -= ONE_FOURTH;
                self.low -= ONE_FOURTH;
                self.high -= ONE_FOURTH;
            } else {
                break;
            }
            self.low <<= 1;
            self.high <<= 1;
            self.high += 1;
            self.value <<= 1;
            let next_bit = src_reader
                .read_bit()
                .map(|b| if b { 1u64 } else { 0u64 })
                .unwrap_or(0u64);
            self.value += next_bit;
        }
        self.src = Some(src_reader);
        return Some(symbol);
    }

    fn eof_symbol(&self) -> u16 {
        self.max_symbol + 1
    }

    pub fn clear(&mut self) {
        self.output_bs.clear();
        self.model.clear();
    }
}

impl Read for AriDecoder {
    /// assuming the output is a symbol stream
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_bits = buf.len() * 8;

        loop {
            // if there are enough bits in the output stream, we are ready to read
            if self.output_bs.bits_in_stream() >= requested_bits {
                return self.output_bs.read(buf);
            }

            // we need more bits
            match self.read_bits_for_symbol() {
                Some(symbol) => {
                    // write the symbol to the output stream, exactly the number of bits per symbol
                    self.output_bs
                        .write_n_bits_u64(self.bits_per_symbol, symbol as u64);
                }
                // EOF
                None => {
                    return self.output_bs.read(buf);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::compression::{Pipeline, mtf::MtfEncoder};

    use super::*;
    #[test]
    fn compress() {
        let in_bytes = b"ijijijygygyguhuhuhijiijbbbbbeeeeewwwwweiiijjijiwoopowiuhiuhoppopownjjnjjnvzcxnvkjnvsjkndnsjknjksvdjkndsvnjkuhbjhbw";
        let mut compressor = AriEncoder::new_adaptive_bytes();
        compressor.attach_reader(Box::new(in_bytes.as_slice()));

        let mut out_bytes = Vec::new();
        let _ = compressor.read_to_end(&mut out_bytes);
        assert_ne!(out_bytes.len(), 0);
        dbg!(in_bytes.len(), out_bytes.len());
        println!();

        let mut p = Pipeline::from_reader(Box::new(in_bytes.as_slice()))
            .pipe(Box::new(MtfEncoder::new()))
            .pipe(Box::new(AriEncoder::new_adaptive_bytes()));

        out_bytes.clear();
        let _ = p.read_to_end(&mut out_bytes);
        dbg!(in_bytes.len(), out_bytes.len());
    }

    #[test]
    fn compress_decompress() {
        let bytestrings = vec![
            Vec::from(b"9"),
            Vec::from(b"ab"),
            Vec::from(b"abc"),
            Vec::from(b"aaa"),
            Vec::from(b"aaaaaab"),
            Vec::from(b"Hello 123"),
            Vec::from(b"oHello 123 Hello 123"),
            Vec::from(b"3pi4ugh4pgph934hfhiuiuhfiouqoiwfooi3riogw3opgw3go34g4i 490rqpiugpiq3 gpiq 3puf piiq3i4 "),
            Vec::from(b"aiourvbpouweghoipwpourohguwo3;gohuiou3qou o3qo4p i42p9 8b19 oias hfwefl wlkrjfnkNLKBKGL IRiaou rpoiue  poi;NP Finpena; oiprgn lhieho;jp lj3knw4gio ;ijeefkvlc;izjv lekjg;l kjwlf jr3oqgn p3i p398240t7092835760934587608934hgiou wbergn;orewir gjweo;rijg o;weijg op4325gj p9245gj p29485jg p93485jg p93485jgp 9384j5gp 98j435p9g j8345p98gj p93458jg p9345jg8 p34958gj 3p459gj8 34p598gj p34598gj p34598gj 4g3p5galejrlakjsfklasjdfkasfjkaskjdfkajsfkwjfkjewfjwogeriugbweiougrboweirubgoweirubgowieurbgowieurbgoiwuerbgoiub"),
            Vec::from(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()1234567890-=_+[]\\{}|"),
            Vec::from(b"-------------------------------------------------------------------------------------------------------------------------------------------------------"),
        ];

        for s in bytestrings {
            println!(
                "\n===============================================================================================\n"
            );
            let mut compressor = AriEncoder::new_adaptive_bytes();
            let mut decompressor = AriDecoder::new_adaptive_bytes();
            let mut compressed_vec: Vec<u8> = Vec::new();
            let mut decompressed_vec: Vec<u8> = Vec::new();
            compressor.attach_reader(Box::new(std::io::Cursor::new(s.clone())));
            let _ = compressor.read_to_end(&mut compressed_vec);

            println!("{} => {}", s.len(), compressed_vec.len());
            println!(
                "Original:   {}\n\nCompressed: {}\n",
                hex::encode(&s),
                hex::encode(&compressed_vec)
            );
            assert!(compressed_vec.len() > 0);

            decompressor.attach_reader(Box::new(std::io::Cursor::new(compressed_vec.clone())));
            let _ = decompressor.read_to_end(&mut decompressed_vec);

            println!("Decompressed: {}\n", hex::encode(&decompressed_vec));

            drop(compressor);
            drop(decompressor);

            assert_eq!(s, decompressed_vec);
        }
    }
}

// /// outputs symbols to symbol stream from the internal bitstream
// /// must prime the bitstream first
// pub fn decode(&mut self, mut input_stream: impl Read, mut out_stream: impl Write) {
//     let mut high = ONE;
//     let mut low: u64 = 0;
//     let mut value: u64 = 0;
//     let _ = self.output_bs.read_n_bits_u64(CODE_BITS as u8, &mut value);
//     value >>= 64 - CODE_BITS;
//     while let Some(symbol) = self.read_bits_for_symbol(&mut value, &mut high, &mut low) {
//         out_stream.write_all(&[symbol]);
//     }
// }
