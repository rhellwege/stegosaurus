mod compression;
use clap::{Arg, Command, Parser};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write, stdin, stdout};
use std::path::PathBuf;

use crate::compression::Pipeline;
use crate::compression::arith::{AriDecoder, AriEncoder};
use crate::compression::mtf::{MtfDecoder, MtfEncoder};

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
        .pipe(Box::new(MtfEncoder::new()))
        .pipe(Box::new(AriEncoder::new_adaptive_bytes()))
}

fn make_decompressor(src: Box<dyn Read>) -> Pipeline {
    Pipeline::from_reader(src)
        .pipe(Box::new(AriDecoder::new_adaptive_bytes()))
        .pipe(Box::new(MtfDecoder::new()))
}

fn main() {
    let cli = Cli::parse();

    let input_stream: BufReader<Box<dyn Read>> = match cli._in {
        Some(path) => BufReader::new(Box::new(File::open(path).unwrap())),
        None => BufReader::new(Box::new(stdin())),
    };

    let mut output_stream: BufWriter<Box<dyn Write>> = match cli.out {
        Some(path) => BufWriter::new(Box::new(File::create(&path).unwrap())),
        None => BufWriter::new(Box::new(stdout())),
    };

    let pipeline = if cli.compress || (!cli.compress && !cli.decompress) {
        make_compressor(Box::new(input_stream))
    } else {
        make_decompressor(Box::new(input_stream))
    };

    for byte in pipeline.bytes() {
        let _ = output_stream.write_all(&[byte.unwrap()]);
    }
}
