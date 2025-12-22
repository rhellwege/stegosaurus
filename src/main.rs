mod compression;
use clap::{Arg, Command, Parser};
use compression::arith;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write, stdin, stdout};
use std::path::PathBuf;

use crate::compression::Compressor;

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

fn main() {
    let cli = Cli::parse();
    let mut compressor = arith::ArithmeticCompressor::new_adaptive();

    let input_stream: BufReader<Box<dyn Read>> = match cli._in {
        Some(path) => BufReader::new(Box::new(File::open(path).unwrap())),
        None => BufReader::new(Box::new(stdin())),
    };

    let output_stream: BufWriter<Box<dyn Write>> = match cli.out {
        Some(path) => BufWriter::new(Box::new(
            File::open(&path).unwrap_or(File::create(path).unwrap()),
        )),
        None => BufWriter::new(Box::new(stdout())),
    };

    if cli.compress || (!cli.compress && !cli.decompress) {
        let _ = compressor.compress(input_stream, output_stream);
    } else {
        let _ = compressor.decompress(input_stream, output_stream);
    }
}
