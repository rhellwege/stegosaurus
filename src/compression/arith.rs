use super::Compressor;
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
    pub fn update_freq(&mut self, symbol: u16) {
        if self.count() as usize >= MAX_FREQ {
            self.clear();
        }
        for i in (symbol as usize + 1)..258 {
            self.cum_freqs[i] += 1;
        }
    }

    pub fn count(&mut self) -> u32 {
        return self.cum_freqs[257];
    }

    pub fn clear(&mut self) {
        for i in 0..258 {
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
        for i in 0u16..257 {
            if scaled_value < self.cum_freqs[(i + 1) as usize] as u64 {
                *symbol = i;
                return Some(self.get_probability(i));
            }
        }
        None
    }
}

pub struct ArithmeticCompressor {
    model: AdaptiveModel,
    bitstream: BitStream,
    pending_bits: usize,
}

impl ArithmeticCompressor {
    pub fn new_adaptive() -> Self {
        ArithmeticCompressor {
            model: AdaptiveModel::new(),
            bitstream: BitStream::new(),
            pending_bits: 0,
        }
    }

    // https://web.archive.org/web/20241113133144/https://marknelson.us/posts/2014/10/19/data-compression-with-arithmetic-coding.html
    fn write_bits_for_symbol(&mut self, symbol: u16, high: &mut u64, low: &mut u64) {
        let p = self.model.get_probability(symbol);

        let range = *high - *low + 1;
        *high = *low + range * p.upper / p.denom - 1;
        *low = *low + range * p.lower / p.denom;

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

    pub fn encode(&mut self, mut source: impl Read, mut dest: impl Write) {
        let mut high = ONE;
        let mut low = 0;

        for next in source.bytes() {
            if let Ok(symbol) = next {
                self.write_bits_for_symbol(symbol as u16, &mut high, &mut low);
            }
        }

        // write eof bits
        self.write_bits_for_symbol(EOF_SYMBOL, &mut high, &mut low);
        // write outstanding bits
        self.pending_bits += 1;
        if low < ONE_FOURTH {
            self.output_bit_plus_pending(false);
        } else {
            self.output_bit_plus_pending(true);
        }
    }

    fn read_bits_for_symbol(
        &mut self,
        value: &mut u64,
        high: &mut u64,
        low: &mut u64,
    ) -> Option<u8> {
        let range = *high - *low + 1;
        let scaled_value = ((*value - *low + 1) * self.model.count() as u64 - 1) / range;
        let mut symbol: u16 = 0;
        let p = self.model.get_symbol(scaled_value, &mut symbol)?;

        if symbol == EOF_SYMBOL {
            return None;
        }
        // before returning the symbol we just got, renormalize high, low, value to prepare the next one
        *high = *low + (range * p.upper) / p.denom - 1;
        *low = *low + (range * p.lower) / p.denom;
        loop {
            if *high < ONE_HALF {
                // if 0, noop
            } else if *low >= ONE_HALF {
                *value -= ONE_HALF; //subtract one half from all three code values
                *low -= ONE_HALF;
                *high -= ONE_HALF;
            } else if *low >= ONE_FOURTH && *high < THREE_FOURTHS {
                *value -= ONE_FOURTH;
                *low -= ONE_FOURTH;
                *high -= ONE_FOURTH;
            } else {
                break;
            }
            *low <<= 1;
            *high <<= 1;
            *high += 1;
            *value <<= 1;
            let next_bit = self
                .bitstream
                .read_bit()
                .map(|b| if b { 1u64 } else { 0u64 })
                .unwrap_or(0u64);
            *value += next_bit;
        }
        return Some(symbol as u8);
    }

    /// outputs symbols to symbol stream from the internal bitstream
    /// must prime the bitstream first
    pub fn decode(&mut self, mut input_stream: impl Read, mut out_stream: impl Write) {
        let mut high = ONE;
        let mut low: u64 = 0;
        let mut value: u64 = 0;
        let _ = self.bitstream.read_n_bits_u64(CODE_BITS as u8, &mut value);
        value >>= 64 - CODE_BITS;
        while let Some(symbol) = self.read_bits_for_symbol(&mut value, &mut high, &mut low) {
            out_stream.write_all(&[symbol]);
        }
    }

    fn output_bit_plus_pending(&mut self, bit: bool) {
        self.bitstream.write_bit(bit);

        while self.pending_bits > 0 {
            self.bitstream.write_bit(!bit);
            self.pending_bits -= 1;
        }
    }

    pub fn clear(&mut self) {
        self.bitstream.clear();
        self.model.clear();
        self.pending_bits = 0;
    }
}

impl Compressor for ArithmeticCompressor {
    fn compress(
        &mut self,
        mut input_stream: impl Read,
        mut output_stream: impl Write,
    ) -> Result<()> {
        self.encode(&mut input_stream, &mut output_stream);
        // TODO: streaming
        let mut bytes = Vec::new();
        self.bitstream.read_to_end(&mut bytes);
        output_stream.write_all(&bytes);
        output_stream.flush()?;
        self.clear();
        Ok(())
    }

    fn decompress(
        &mut self,
        mut input_stream: impl Read,
        mut output_stream: impl Write,
    ) -> Result<()> {
        // read the whole input stream
        let mut input_bytes = Vec::new();
        let _ = input_stream.read_to_end(&mut input_bytes)?;
        // prime the decoder bitstream
        self.bitstream.write_all(&input_bytes)?;
        self.decode(input_stream, output_stream);
        self.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compress() {
        let in_bytes = b"babcghabcdefghijabcdef qpwur voqui rw vpqv p93 v9q v9384 v09w83 v098w v89w ofvadf vs d fvkjsldkfjv lksdjb lksklw tlke g  gkjw eklg  g kwegk wergj k wegkjlwk jeg kjrjkeg jwe kgjk wegkj wek jlg jkwekjrg jkwk jk jk jk j kj kjw jk kjvk kj k jwkjve rkjghklmnopqrstuvwxyz";
        let mut compressor = ArithmeticCompressor::new_adaptive();

        let mut out_bytes = Vec::new();
        let _ = compressor
            .compress(in_bytes.as_slice(), &mut out_bytes)
            .unwrap();
        // dbg!(&out_bytes, out_bytes.len());
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

        let mut compressor = ArithmeticCompressor::new_adaptive();
        let mut compressed_vec: Vec<u8> = Vec::new();
        let mut decompressed_vec: Vec<u8> = Vec::new();

        for s in bytestrings {
            println!(
                "\n===============================================================================================\n"
            );
            let _ = compressor
                .compress(s.as_slice(), &mut compressed_vec)
                .unwrap();
            println!("{} => {}", s.len(), compressed_vec.len());
            println!(
                "Original:   {}\n\nCompressed: {}\n",
                hex::encode(&s),
                hex::encode(&compressed_vec)
            );
            assert!(compressed_vec.len() > 0);

            let _ = compressor
                .decompress(compressed_vec.as_slice(), &mut decompressed_vec)
                .unwrap();

            println!("Decompressed: {}\n", hex::encode(&decompressed_vec));

            assert_eq!(s, decompressed_vec);
            compressed_vec.clear();
            decompressed_vec.clear();
        }
    }
}
