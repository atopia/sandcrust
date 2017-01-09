extern crate sandcrust;
extern crate libc;


use sandcrust::*;
use std::ffi::CString;
use libc::*;

#[link="c"]
extern {
	fn snprintf(str: *mut c_char, size: size_t, format: *const c_char, ...) -> c_int;
}

/*
fn sprintf1(format: &str, value: ???) -> String {
	const BUF_SIZE: usize = 256;
	unsafe {
			let format = CString::new(format).unwrap().into_raw();
			let buffer = CString::new(&Vec::with_capacity(BUF_SIZE)).unwrap().into_raw();

			snprintf(format, BUF_SIZE as size_t, buffer, value);

			let result = CString::from_raw(buffer);
			from_utf8(result).unwrap().to_string()
		}
}
*/



fn main() {
	let size: size_t = 256;
	let formatstr = CString::new("I am %s, of %d years\n").unwrap();
	let mut vec = Vec::with_capacity(size);

	let namestr = CString::new("Ben").unwrap();
	let age: c_uint = 31;
	unsafe {
		let fmt = formatstr.as_ptr();
		let name = namestr.as_ptr();
		let buf = vec.as_mut_ptr() as *mut i8;
		let len = snprintf(buf, size, fmt, name, age);
		vec.set_len(len as usize);
//		let result = CString::from_raw(buf);
	}
	let stringy = String::from_utf8(vec).unwrap();
	println!("string is {}", stringy);
}
