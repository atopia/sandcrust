#[macro_use]
extern crate sandcrust;

use sandcrust::*;

sandbox!{
	fn empty() {
		println!("so empty!");
	}
}

sandbox!{
	fn Add() {
		println!("so additional!");
	}
}

sandbox!{
	fn half(bla: i32) {
		println!("so full with {}!", bla);
	}
}

sandbox!{
	fn full(bla: i32, blubb: i64) {
		println!("so full with {} and {}!", bla, blubb);
	}
}

sandbox!{
	fn base_inc(a: &mut i32) {
		*a += 1;
	}
}

sandbox!{
	fn base_ret() -> i32 {
		let ret = 23;
		ret
	}
}

sandbox!{
fn sandcrust_bincode(src: &[u8]) -> Vec<u8> {
	//let refer = &src[..];
	let new = src.to_vec();
	new
}
}

fn main() {

	let vector = vec![23u8; 42];
	let new = sandcrust_bincode(&vector);
	assert_eq!(vector, new);
	Add();
	println!("before init");
	sandcrust_init();
	println!("after init");
	half(32);
	sandcrust_terminate();
	println!("after terminate");
	full(32, 1);
	let mut a: i32 = 23;
	println!("after auto-launch");
	sandcrust_init();
	println!("after re-launch");
	base_inc(&mut a);
	assert_eq!(a, 24);
	let local_ret = base_ret();
	assert_eq!(local_ret, 23);
	empty();
}
