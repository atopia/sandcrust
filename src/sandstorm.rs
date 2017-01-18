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

fn snprintf_wrapper(vec: &mut Vec<u8>, size: size_t, format: *const c_char, name: *const c_char, age: c_uint, len: &mut c_int) -> i32 {
    println!("wrapped: PID: {}", nix::unistd::getpid());
    let status = 12;
	unsafe {
		let buf = vec.as_mut_ptr() as *mut i8;
		let lenny = snprintf(buf, size, format, name, age);
		vec.set_len(lenny as usize);
        *len = lenny;
	}
    status
}


fn base_ret() -> i32 {
    let ret = 23;
    ret
}

fn second_base_ret(bla: &mut i32) -> i32 {
    let ret = 23;
    *bla = 7;
    ret
}

fn empty() {
    ;
}

fn main() {
    sandbox_no_ret!(empty());
    println!(">>> actually continue after empty");
    let val1 = sandbox!(base_ret());
    let mut bla = 22;
    let val2 = sandbox!(second_base_ret(&mut bla));
    assert!(bla == 7);
    assert!(23 == val1);
    assert!(23 == val2);
	let size: size_t = 256;
	let formatstr = CString::new("I am %s, of %d years\n").unwrap();
	let mut vec = Vec::with_capacity(size);

	let namestr = CString::new("Ben").unwrap();
	let fmt = formatstr.as_ptr();
	let name = namestr.as_ptr();
	let age: c_uint = 31;
    println!("orig: PID: {}", nix::unistd::getpid());
    let mut len: c_int = 0;
    let status: c_int = sandbox!(snprintf_wrapper(&mut vec, size, fmt, name, age, &mut len));
	let stringy = String::from_utf8(vec).unwrap();
	println!("string is {} with new len {} and status {}", stringy, len, status);
}
