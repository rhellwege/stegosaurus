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
        for i in (0..s.len() - 2).rev() {
            t[i] = (s[i] < s[i + 1]) || (s[i] == s[i + 1] && t[i + 1]);
        }
        t[s.len()] = true;

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
    dbg!(&sorted_suffixes);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sais_test() {
        let s = b"mmiiabscbnnenwgorigmrimskcv,smdklrkgmer s.v serv mmer vme mv msevr ,mer vme slkrjnglkjrgkej b sejb kje skbj krje skjb rkjleeskr9guw09-40f934f094309034f0s9fv09snv09sn 09 90s90j 09j90j990rewb90j0bwroibpweriwbpiowrgjpk'fwor;f;oqwrfowoijiio iooij io iioj ioiojfliwqbfniwqefiowequbfiwbeqioufbiubuiioiuobuiiubuoiubuiuibobuissiissiippii";
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
        // dbg!(bwt(s));
    }

    #[test]
    fn inverse() {
        //        0123456789
        let s = b"PINEAPPLE";
        let s = b"mmiiabsciamgreaitifo fpaimfiamgreatifobnnenwgorigmrimskcv,smdklrkgmer s.v serv mmer vme mv msevr ,mer vme slkrjnglkjrgkej b sejb kje skbj krje skjb rkjleeskr9guw09-40f934f094309034f0s9fv09snv09sn 09 90s90j 09j90j990rewb90j0bwroibpweriwbpiowrgjpk'fwor;f;oqwrfowoijiio iooij io iioj ioiojfliwqbfniwqefiowequbfiwbeqioufbiubuiioiuobuiiubuoiubuiuibobuissiissiippii";
        let (bwt, original) = bwt(s);
        let out = inverse_bwt(&bwt, original);
        assert_eq!(s, out.as_slice());
    }
}
