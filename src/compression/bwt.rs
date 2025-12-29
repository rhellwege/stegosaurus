use std::io::{BufRead, Cursor, Read, Write};

use num::ToPrimitive;

use crate::compression::{DataTransform, bitstream::BitStream};

// https://zork.net/~st/jottings/sais.html
mod sais {
    pub fn is_lms(t: &[bool], i: usize) -> bool {
        i != 0 && (t[i] && !t[i - 1])
    }

    pub fn bucket_sizes(s: &[i32], alphabet_size: u32) -> Vec<u32> {
        let mut b = vec![0; alphabet_size as usize];
        for c in s {
            b[*c as usize] += 1;
        }
        b
    }

    pub fn bucket_heads(bucket_sizes: &[u32]) -> Vec<i32> {
        let mut i = 1i32;
        let mut res = vec![-1i32; bucket_sizes.len()];
        for (index, size) in bucket_sizes.iter().enumerate() {
            res[index] = i;
            i += *size as i32;
        }
        res
    }

    pub fn bucket_tails(bucket_sizes: &[u32]) -> Vec<i32> {
        let mut i = 1i32;
        let mut res = vec![-1i32; bucket_sizes.len()];
        for (index, size) in bucket_sizes.iter().enumerate() {
            i += *size as i32;
            res[index] = i - 1;
        }
        res
    }

    pub fn guess_sa_lms_sort(s: &[i32], bucket_sizes: &[u32], t: &[bool]) -> Vec<i32> {
        let mut sa = vec![-1i32; s.len() + 1];
        let mut tails = bucket_tails(bucket_sizes);

        // bucket sort lms suffixes
        for i in 0..s.len() {
            // ignore non-lms
            if !is_lms(t, i) {
                continue;
            }

            // add this prefix to the tail of its bucket (s[i])
            let bucket = s[i] as usize;
            sa[tails[bucket] as usize] = i as i32;
            tails[bucket] -= 1;
        }
        // The empty suffix is defined to be an LMS-substring, and we know it goes to the front
        sa[0] = s.len() as i32;
        sa
    }

    // slot in l type suffixes into the guessed sa
    pub fn induce_sort_l(s: &[i32], guessed_sa: &mut [i32], bucket_sizes: &[u32], t: &[bool]) {
        let mut bucket_heads = bucket_heads(bucket_sizes);

        for i in 0..guessed_sa.len() {
            if guessed_sa[i] == -1 {
                continue;
            }

            // we are interested in the entry to the left of the guessed suffix
            let j = guessed_sa[i] - 1;

            // there is no suffix to the left of the first suffix
            if j < 0 {
                continue;
            }

            // we are only interested in l types
            if t[j as usize] {
                continue;
            }

            let bucket = s[j as usize] as usize;
            // put the suffix in the head of its bucket
            guessed_sa[bucket_heads[bucket] as usize] = j;
            bucket_heads[bucket] += 1;
        }
    }
    // slot in s type suffixes into the guessed sa right to left
    pub fn induce_sort_s(s: &[i32], guessed_sa: &mut [i32], bucket_sizes: &[u32], t: &[bool]) {
        let mut bucket_tails = bucket_tails(bucket_sizes);

        for i in (0..guessed_sa.len()).rev() {
            let j = guessed_sa[i] - 1;

            // there is no suffix to the left of the first suffix
            if j < 0 {
                continue;
            }

            // we are only interested in s types
            if !t[j as usize] {
                continue;
            }

            let bucket = s[j as usize] as usize;
            // put the suffix in the tail of its bucket
            guessed_sa[bucket_tails[bucket] as usize] = j;
            bucket_tails[bucket] -= 1;
        }
    }

    // are lms substrings equal
    fn lms_eq(s: &[i32], t: &[bool], a: usize, b: usize) -> bool {
        if a == s.len() || b == s.len() {
            return false;
        }

        let mut i = 0;
        loop {
            let a_is_lms = is_lms(t, a + i);
            let b_is_lms = is_lms(t, b + i);

            // we've iterated through both strings entirely
            if i > 0 && a_is_lms && b_is_lms {
                return true;
            }

            // one substring ended
            if a_is_lms != b_is_lms {
                return false;
            }

            // characters must be equal
            if s[a + i] != s[b + i] {
                return false;
            }

            i += 1
        }
    }

    // produces a summary of the positions of lms suffixes
    /// returns (summary_string, summary_alphabet_size, summary_suffix_offsets)
    fn summarise_sa(s: &[i32], guessed_sa: &[i32], t: &[bool]) -> (Vec<i32>, u32, Vec<i32>) {
        let mut lms_names = vec![-1i32; s.len() + 1];

        // keep track of names we've allocated so far.
        let mut current_name = 0i32;

        // We know that the first LMS-substring we'll see will always be
        // the one representing the empty suffix, and it will always be at
        // position 0 of suffixOffset.
        lms_names[guessed_sa[0] as usize] = current_name;
        // Where in the original string was the last LMS suffix we checked?
        let mut last_lms_offset = guessed_sa[0];

        for i in 1..guessed_sa.len() {
            // where does this suffix appear in the original string?
            let suffix_offset = guessed_sa[i];

            // we only care about lms suffixes
            if !is_lms(t, suffix_offset as usize) {
                continue;
            }

            // assign a new name if the lms substring is not equal to the last one
            if !lms_eq(s, t, last_lms_offset as usize, suffix_offset as usize) {
                current_name += 1;
            }

            // record the suffix we just looked at:
            last_lms_offset = suffix_offset;

            // store the name of this LMS suffix in lmsNames, in the same
            // place this suffix occurs in the original string.
            lms_names[suffix_offset as usize] = current_name;
        }

        // now lmsNames contains all the characters of the suffix string in
        // the correct order, but it also contains a lot of unused indexes
        // we don't care about and which we want to remove
        let mut summary_suffix_offsets = Vec::new();
        let mut summary_string = Vec::new();

        for (i, name) in lms_names.iter().enumerate() {
            if *name == -1 {
                continue;
            }

            summary_suffix_offsets.push(i as i32);
            summary_string.push(*name);
        }

        let summary_alphabet_size = current_name + 1;

        (
            summary_string,
            summary_alphabet_size as u32,
            summary_suffix_offsets,
        )
    }

    fn make_summary_sa(summary_string: &[i32], summary_alphabet_size: u32) -> Vec<i32> {
        if summary_alphabet_size == summary_string.len() as u32 {
            // every character of this summary string appears once and only
            // once, so we can make the suffix array with a bucket sort.
            let mut summary_sa = vec![-1i32; summary_string.len() + 1];
            // always include the empty suffix at the beginning
            summary_sa[0] = summary_string.len() as i32;
            for i in 0..summary_string.len() {
                let j = summary_string[i];
                summary_sa[j as usize + 1] = i as i32;
            }
            summary_sa
        } else {
            sais_i32(summary_string, summary_alphabet_size)
        }
    }

    fn accurate_lms_sort(
        s: &[i32],
        bucket_sizes: &[u32],
        t: &[bool],
        summary_sa: &[i32],
        summary_suffix_offsets: &[i32],
    ) -> Vec<i32> {
        let mut sa = vec![-1i32; s.len() + 1];

        let mut bucket_tails = bucket_tails(bucket_sizes);
        for i in (2..summary_sa.len()).rev() {
            let string_index = summary_suffix_offsets[summary_sa[i] as usize] as usize;
            // if string_index as usize >= s.len() {
            //     continue;
            // }

            let bucket = s[string_index];

            // add the suffix to the tail of the bucket
            sa[bucket_tails[bucket as usize] as usize] = string_index as i32;

            bucket_tails[bucket as usize] -= 1;
        }

        sa[0] = s.len() as i32;
        sa
    }

    fn sais_i32(s: &[i32], alphabet_size: u32) -> Vec<i32> {
        // Step 1: classify sufixes as S-type or L-type
        let mut t = vec![false; s.len() + 1]; // is S-type
        t[s.len()] = true;
        for i in (0..s.len() - 1).rev() {
            t[i] = (s[i] < s[i + 1]) || (s[i] == s[i + 1] && t[i + 1]);
        }

        let bucket_sizes = bucket_sizes(s, alphabet_size);
        let mut guessed_sa = guess_sa_lms_sort(s, &bucket_sizes, &t);

        induce_sort_l(s, &mut guessed_sa, &bucket_sizes, &t);
        induce_sort_s(s, &mut guessed_sa, &bucket_sizes, &t);

        // create a new string that summarises the relative order of LMS
        // suffixes in the guessed suffix array.
        let (summary_string, summary_alphabet_size, summary_suffix_offsets) =
            summarise_sa(s, &guessed_sa, &t);

        // make a sorted suffix array of the summary string.
        let summary_sa = make_summary_sa(&summary_string, summary_alphabet_size);

        // using the suffix array of the summary string, determine exactly
        // where the LMS suffixes should go in our final array.
        let mut result =
            accurate_lms_sort(s, &bucket_sizes, &t, &summary_sa, &summary_suffix_offsets);

        induce_sort_l(s, &mut result, &bucket_sizes, &t);
        induce_sort_s(s, &mut result, &bucket_sizes, &t);
        result
    }

    pub fn sais(s: &[u8]) -> Vec<i32> {
        let s = s.iter().map(|x| *x as i32).collect::<Vec<i32>>();
        sais_i32(&s, 256)
    }
}

fn slow_sa(s: &[u8]) -> Vec<i32> {
    let mut out = vec![-1i32; s.len()];
    for i in 0..out.len() {
        out[i] = i as i32;
    }

    out.sort_by(|a, b| s[*a as usize..].cmp(&s[*b as usize..]));

    out
}

pub fn bwt(s: &[u8]) -> (Vec<u8>, usize) {
    if s.len() == 1 {
        return (s.to_vec(), 0);
    }
    let mut output = vec![0u8; s.len()];
    // hack to get cyclic ordering from suffix array
    // TODO: make a modified sais algorithm that takes into account cycles
    let mut s_doubled = s.to_vec();
    s_doubled.extend_from_slice(s);
    let sorted_suffixes: Vec<i32> = sais::sais(&s_doubled)
        .into_iter()
        .skip(1)
        .filter(|&i| (i as usize) < s.len())
        .collect();

    // println!("    len {}", sorted_suffixes.len());
    // println!("    slen {}", s.len());
    // let sorted_suffixes = slow_sa(s);
    let mut original: usize = 0;

    for i in 0..sorted_suffixes.len() {
        let sorted = sorted_suffixes[i];
        if sorted == 0 {
            original = i;
        }
        let bwt_index = (sorted as usize + s.len() - 1) % s.len();
        output[i] = s[bwt_index];
    }

    (output, original)
}

fn bucket_sizes_u8(bytes: &[u8]) -> Vec<u32> {
    let mut out = vec![0u32; 256];
    for b in bytes {
        out[*b as usize] += 1;
    }
    out
}

fn bucket_heads(bucket_sizes: &[u32]) -> Vec<u32> {
    let mut i = 0u32;
    let mut res = vec![0u32; bucket_sizes.len()];
    for (byte, size) in bucket_sizes.iter().enumerate() {
        res[byte] = i;
        i += *size as u32;
    }
    res
}

pub fn inverse_bwt(last_column: &[u8], original_idx: usize) -> Vec<u8> {
    if last_column.len() == 1 {
        return last_column.to_vec();
    }
    let mut output = vec![0u8; last_column.len()];
    let mut mapping = vec![0u32; last_column.len()]; // first_column -> last_column

    // Step 1: generate the mapping
    // note: last_column[i] precedes first_column[i] in the string
    // note: both are lexicographically sorted
    // use buckets
    let bucket_sizes = bucket_sizes_u8(last_column);
    let mut bucket_heads = bucket_heads(&bucket_sizes);
    let mut from_buckets = bucket_heads.clone();
    let mut to_buckets = bucket_heads.clone();

    let mut sorted = vec![0u8; last_column.len()];
    for i in 0..last_column.len() {
        let b = last_column[i as usize];
        sorted[bucket_heads[b as usize] as usize] = b;
        bucket_heads[b as usize] += 1;
    }

    // consider last_column, first_column pairs
    for i in 0..last_column.len() {
        let x = last_column[i];
        let y = sorted[i];

        let x_bucket = from_buckets[x as usize];
        from_buckets[x as usize] += 1;

        let y_bucket = to_buckets[y as usize];
        to_buckets[y as usize] += 1;

        mapping[x_bucket as usize] = y_bucket as u32;
    }

    let mut cur = mapping[original_idx] as u32;
    for i in 0..last_column.len() {
        output[i] = last_column[cur as usize];
        cur = mapping[cur as usize];
    }

    output
}

pub struct BwtEncoder {
    src: Option<Box<dyn Read>>,
    payload_transform: Option<Box<dyn DataTransform>>,
    output_bs: BitStream,
    block_size: u32,
    original_index_bits: u8,
}

impl DataTransform for BwtEncoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = Some(src);
    }
}

impl BwtEncoder {
    /// Will use the minimal number of bits possible given the block size
    pub fn new(block_size: u32, original_index_bits: u8) -> Self {
        BwtEncoder {
            src: None,
            payload_transform: None,
            output_bs: BitStream::new(),
            block_size: block_size, // size of the input needed for one block
            original_index_bits: original_index_bits,
        }
    }
}

impl Read for BwtEncoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let requested_bits = buf.len() * 8;
        let mut src_reader = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        let mut input_buf = vec![0u8; self.block_size as usize];

        while self.output_bs.bits_in_stream() < requested_bits {
            let nread = src_reader.read(&mut input_buf)?;
            if nread == 0 {
                self.src = Some(src_reader);
                return self.output_bs.read(buf);
            }
            let (bwt, original_index) = bwt(&input_buf[0..nread]);
            self.output_bs
                .write_n_bits_u64(self.original_index_bits, original_index as u64);
            self.output_bs.write(&bwt);
        }

        self.src = Some(src_reader);
        self.output_bs.read(buf)
    }
}

pub struct BwtDecoder {
    src: Option<BitStream>,
    payload_transform: Option<Box<dyn DataTransform>>,
    output_buffer: Cursor<Vec<u8>>,
    block_size: u32,
    nblocks: usize, // number of blocks decoded
    original_index_bits: u8,
}

impl DataTransform for BwtDecoder {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        let mut bs = BitStream::new();
        bs.attach_reader(src);
        self.src = Some(bs);
    }
}

impl BwtDecoder {
    /// Will use the minimal number of bits possible given the block size
    pub fn new(block_size: u32, original_index_bits: u8) -> Self {
        BwtDecoder {
            src: None,
            payload_transform: None,
            output_buffer: Cursor::new(Vec::new()),
            block_size: block_size, // size of the input needed for one block
            nblocks: 0,
            original_index_bits: original_index_bits,
        }
    }
}

impl Read for BwtDecoder {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut bs = match self.src.take() {
            Some(s) => s,
            None => return Ok(0),
        };

        let mut nread = self.output_buffer.read(buf)?;
        let mut bwt_input_buf = vec![0u8; self.block_size as usize];

        while nread < buf.len() {
            let mut original_index: u64 = 0;
            let bits_read = bs
                .read_n_bits_u64(self.original_index_bits, &mut original_index)
                .map_err(|_| std::io::Error::other("failed to read original index in bwt block"))?;
            if bits_read == 0 {
                self.src = Some(bs);
                return Ok(nread);
            }
            if bits_read < self.original_index_bits as usize {
                self.src = Some(bs);
                return Err(std::io::Error::other(
                    "unexpected end of message, failed to read original index for bwt",
                ));
            }

            let mut bwt_nread = bs.read(&mut bwt_input_buf)?;
            self.nblocks += 1;
            let leftover_bits = (self.nblocks * self.original_index_bits as usize) % 8;
            let mut peek_buf = 0u8;
            let npeeked = bs
                .peek_byte(&mut peek_buf)
                .map_err(|_| std::io::Error::other("failed to peek at bitstream"))?;

            // we need to peek at one more byte
            if npeeked == 0 || bwt_nread < self.block_size as usize {
                // we are at the last bwt block
                // We know that the last block had to output a partial byte
                if leftover_bits != 0 {
                    bwt_nread -= 1;
                    let temp = bwt_input_buf[bwt_nread - 1] & (0xff << leftover_bits);
                    bwt_input_buf[bwt_nread - 1] = temp
                        | (bwt_input_buf[bwt_nread - 1] << (8 - leftover_bits))
                        | bwt_input_buf[bwt_nread];
                }
            }

            // we are at the end of the stream, but there are leftover bits
            if npeeked > 0 && npeeked < 8 {
                if npeeked != (8 - leftover_bits) {
                    self.src = Some(bs);
                    return Err(std::io::Error::other(
                        "unexpected end of message, failed to decode the last block",
                    ));
                }
                // actually read the bits now
                let nread = bs
                    .read_byte(&mut peek_buf)
                    .map_err(|_| std::io::Error::other("failed to final bits"))?;

                let temp = bwt_input_buf[self.block_size as usize - 1] & (0xff << leftover_bits);
                bwt_input_buf[self.block_size as usize - 1] = temp
                    | (bwt_input_buf[self.block_size as usize - 1] << (8 - leftover_bits))
                    | peek_buf;
            }

            let original = inverse_bwt(&bwt_input_buf[0..bwt_nread], original_index as usize);

            let pos = self.output_buffer.position();
            let nwrite = self.output_buffer.write(&original)?;
            self.output_buffer.set_position(pos);
            nread += self.output_buffer.read(&mut buf[nread..])?;
        }

        self.src = Some(bs);
        Ok(nread)
    }
}

#[cfg(test)]
mod tests {
    use crate::compression::Pipeline;

    use super::*;

    #[test]
    fn sais_test() {
        let s = b"mmiiabscbnnenwgorigmrimskcv,smdklrkgmer s.v serv mmer vme mv msevr ,mer vme slkrjnglkjrgkej b sejb kje skbj krje skjb rkjleeskr9guw09-40f934f094309034f0s9fv09snv09sn 09 90s90j 09j90j990rewb90j0bwroibpweriwbpiowrgjpk'fwor;f;oqwrfowoijiio iooij io iioj ioiojfliwqbfniwqefiowequbfiwbeqioufbiubuiioiuobuiiubuoiubuiuibobuissiissiippii";
        let s = &[
            2, 1, 4, 0, 2, 1,
            3, //, 1, 0, 1, //, 112, 105, 117, 103, 112, 105, 113, 51, 32, 103,
        ];
        let sa = &sais::sais(s.as_slice())[1..];
        let sa1 = slow_sa(s.as_slice());
        assert_eq!(sa, sa1);
    }

    #[test]
    fn mississippi() {
        let s = b"mmiiabscbnnenwgorigmrimskcv,smdklrkgmer s.v serv mmer vme mv msevr ,mer vme slkrjnglkjrgkej b sejb kje skbj krje skjb rkjleeskr9guw09-40f934f094309034f0s9fv09snv09sn 09 90s90j 09j90j990rewb90j0bwroibpweriwbpiowrgjpk'fwor;f;oqwrfowoijiio iooij io iioj ioiojfliwqbfniwqefiowequbfiwbeqioufbiubuiioiuobuiiubuoiubuiuibobuissiissiippii";
        let sa = sais::sais(s.as_slice());
        for n in sa.iter() {
            dbg!(&s[*n as usize..]);
        }
        dbg!(&sa);
        for i in 1..sa.len() {
            println!("{}", s[sa[i] as usize]);
        }
    }

    #[test]
    fn inverse() {
        //        0123456789
        let s = b"PINEAPPLE";
        let s = b"mmiiabsciamgreaitifo fpaimfiamgreatifobnnenwgorigmrimskcv,smdklrkgmer s.v serv mmer vme mv msevr ,mer vme slkrjnglkjrgkej b sejb kje skbj krje skjb rkjleeskr9guw09-40f934f094309034f0s9fv09snv09sn 09 90s90j 09j90j990rewb90j0bwroibpweriwbpiowrgjpk'fwor;f;oqwrfowoijiio iooij io iioj ioiojfliwqbfniwqefiowequbfiwbeqioufbiubuiioiuobuiiubuoiubuiuibobuissiissiippii";
        let s = b"3pi4ugh4pgph934hfhiuiuhfiouqoiwfooi3riogw3opgw3go34g4i 490rqpiugpiq3 gpiq 3puf piiq3i4 ";
        let (bwt, original_idx) = bwt(s);
        let out = inverse_bwt(&bwt, original_idx);
        dbg!(s, bwt);
        assert_eq!(s, out.as_slice());
    }

    #[test]
    fn stream() {
        let bytestrings = vec![
            Vec::from(b"3pi4ugh4pgph934hfhiuiuhfiouqoiwfooi3riogw3opgw3go34g4i 490rqpiugpiq3 gpiq 3puf piiq3i4 "),
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
            let mut bwt_bits = Vec::new();
            let _ = Pipeline::from_reader(Box::new(Cursor::new(s.clone())))
                .pipe(Box::new(BwtEncoder::new(10, 8)))
                .read_to_end(&mut bwt_bits);

            let mut original = Vec::new();

            let _ = Pipeline::from_reader(Box::new(Cursor::new(bwt_bits.clone())))
                .pipe(Box::new(BwtDecoder::new(10, 8)))
                .read_to_end(&mut original);

            assert_eq!(&s, original.as_slice());
        }
    }
}
