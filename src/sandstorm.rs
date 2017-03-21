#[macro_use]
extern crate sandcrust;

use std::thread;
use std::time::Duration;

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

fn main() {
	Add();
	half(32);
	full(32, 1);
	let mut a: i32 = 23;
	base_inc(&mut a);
	assert_eq!(a, 24);
	let local_ret = base_ret();
	assert_eq!(local_ret, 23);
	empty();

	println!("Running work for 5 seconds.");
	println!("Can you send a signal quickly enough?");
	thread::sleep(Duration::from_secs(5));

	sandcrust_terminate();
}
