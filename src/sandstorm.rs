extern crate sandcrust;
extern crate libc;
extern crate nix;

use sandcrust::*;
use std::ffi::CString;
use libc::*;

#[link="c"]
extern {
	fn snprintf(str: *mut c_char, size: size_t, format: *const c_char, ...) -> c_int;
}

fn snprintf_wrapper(vec: &mut Vec<u8>, size: size_t, format: *const c_char, name: *const c_char, age: c_uint, len: &mut c_int) {
    println!("wrapped: PID: {}", nix::unistd::getpid());
	unsafe {
		let buf = vec.as_mut_ptr() as *mut i8;
		let lenny = snprintf(buf, size, format, name, age);
		vec.set_len(lenny as usize);
        *len = lenny;
	}
}

fn main() {
	let size: size_t = 256;
	let formatstr = CString::new("I am %s, of %d years\n").unwrap();
	let mut vec = Vec::with_capacity(size);

	let namestr = CString::new("Ben").unwrap();
	let fmt = formatstr.as_ptr();
	let name = namestr.as_ptr();
	let age: c_uint = 31;
    println!("orig: PID: {}", nix::unistd::getpid());
    let mut len: c_int = 0;
    sandbox_me!(snprintf_wrapper(&mut vec, size, fmt, name, age, &mut len));
	let stringy = String::from_utf8(vec).unwrap();
	println!("string is {} with new len {}", stringy, len);
}
