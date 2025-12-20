mod compression;
use compression::arith;

fn main() {
    let a = arith::ArithmeticCompressor::new();
    println!("Hello, world!");
}
