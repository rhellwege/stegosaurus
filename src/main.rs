mod compression;
use clap::{Arg, Command, Parser};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write, stdin, stdout};
use std::path::PathBuf;

use crate::compression::Pipeline;
use crate::compression::arith::{AriDecoder, AriEncoder};
use crate::compression::bwt::{BwtDecoder, BwtEncoder};
use crate::compression::bzrle::{BzrleDecoder, BzrleEncoder};
use crate::compression::mtf::{MtfDecoder, MtfEncoder};
use crate::compression::rle::{RleDecoder, RleEncoder};

#[derive(Parser, Debug)]
struct Cli {
    /// If specified reads the file otherwise use stdin
    #[arg(short, long)]
    _in: Option<PathBuf>,

    /// If specified writes to the file otherwise use stdout
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Compress the input stream to the output stream
    /// Implied if neither compress nor decompress are specified.
    #[arg(short, long)]
    compress: bool,

    /// Decompress the input stream to the output stream
    #[arg(short, long)]
    decompress: bool,
}

fn make_compressor(src: Box<dyn Read>) -> Pipeline {
    Pipeline::from_reader(src)
        .pipe(Box::new(BwtEncoder::new(2u32.pow(24), 24)))
        .pipe(Box::new(MtfEncoder::new()))
        .pipe(Box::new(BzrleEncoder::new(0, 256, 16)))
        .pipe(Box::new(AriEncoder::new_adaptive(16, 256)))
}

fn make_decompressor(src: Box<dyn Read>) -> Pipeline {
    Pipeline::from_reader(src)
        .pipe(Box::new(AriDecoder::new_adaptive(16, 256)))
        .pipe(Box::new(BzrleDecoder::new(0, 256, 16)))
        .pipe(Box::new(MtfDecoder::new()))
        .pipe(Box::new(BwtDecoder::new(2u32.pow(24), 24)))
}

fn main() {
    let cli = Cli::parse();

    let input_stream: Box<dyn Read> = match cli._in {
        Some(path) => Box::new(File::open(path).unwrap()),
        None => Box::new(stdin()),
    };

    let mut output_stream: Box<dyn Write> = match cli.out {
        Some(path) => Box::new(File::create(&path).unwrap()),
        None => Box::new(stdout()),
    };

    let mut pipeline = if cli.compress || (!cli.compress && !cli.decompress) {
        make_compressor(Box::new(input_stream))
    } else {
        make_decompressor(Box::new(input_stream))
    };

    let mut output: Vec<u8> = Vec::new();

    pipeline.read_to_end(&mut output);

    output_stream.write_all(&output);
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::{BufReader, Cursor, Read},
    };

    use crate::{
        compression::{
            Pipeline,
            arith::{AriDecoder, AriEncoder},
            bwt::{BwtDecoder, BwtEncoder},
            mtf::{MtfDecoder, MtfEncoder},
        },
        make_compressor, make_decompressor,
    };

    #[test]
    fn e2e() {
        let tests_dir = std::env::current_dir().unwrap().join("tests"); //.join("../tests");
        for entry in std::fs::read_dir(tests_dir).unwrap() {
            let entry = entry.unwrap();
            let file_path = entry.path();
            println!(
                "-----------------------------------------------------------------------------"
            );
            println!("{}", file_path.to_string_lossy());
            let src: Vec<u8> = File::open(&file_path)
                .unwrap()
                .bytes()
                .map(|r| r.unwrap())
                .collect();
            let mut comp = make_compressor(Box::new(std::io::Cursor::new(src.clone())));
            let mut compressed: Vec<u8> = Vec::new();
            comp.read_to_end(&mut compressed).unwrap();
            println!(
                "{} ==> {} {}%",
                src.len(),
                compressed.len(),
                100.0 * (compressed.len() as f64 / src.len() as f64)
            );
            let mut output: Vec<u8> = Vec::new();
            let mut decomp = make_decompressor(Box::new(std::io::Cursor::new(compressed)));
            decomp.read_to_end(&mut output).unwrap();
            assert!(src == output, "Compressor did not invert!");
        }
    }

    #[test]
    fn bwt_encode_decode() {
        let block_bits: Vec<(u32, u8)> = vec![
            (10, 4),
            (16, 4),
            (100, 7),
            (100, 8),
            (100, 9),
            (20_000, 15),
            (20_000, 16),
            (100_000, 24),
            (100_000, 19),
            (900_000, 24),
            (900_000, 20),
            (2_000_000, 22),
        ];

        for (block_size, bits_per_idx) in block_bits.into_iter().rev() {
            println!(
                "============================================================================="
            );
            dbg!(block_size, bits_per_idx);
            let tests_dir = std::env::current_dir().unwrap().join("tests"); //.join("../tests");
            for entry in std::fs::read_dir(tests_dir).unwrap() {
                let entry = entry.unwrap();
                let file_path = entry.path();
                println!(
                    "-----------------------------------------------------------------------------"
                );
                println!("{}", file_path.to_string_lossy());
                let src: Vec<u8> = File::open(&file_path)
                    .unwrap()
                    .bytes()
                    .map(|r| r.unwrap())
                    .collect();

                if src.len() > 100000 {
                    continue;
                }

                let result: Vec<u8> = Pipeline::from_reader(Box::new(Cursor::new(src.clone())))
                    .pipe(Box::new(BwtEncoder::new(block_size, bits_per_idx)))
                    .pipe(Box::new(MtfEncoder::new()))
                    .pipe(Box::new(AriEncoder::new_adaptive_bytes()))
                    .pipe(Box::new(AriDecoder::new_adaptive_bytes()))
                    .pipe(Box::new(MtfDecoder::new()))
                    .pipe(Box::new(BwtDecoder::new(block_size, bits_per_idx)))
                    .bytes()
                    .map(|r| r.unwrap())
                    .collect();

                assert!(result == src, "Bwt did not invert correctly");
            }
        }
    }
}
