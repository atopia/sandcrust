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

macro_rules! handle_args {
    ($head:ident : $typo:ty) => {
        let newvar: &$typo = &$head;
        println!("newvar is : {}", $head);
    };
    ($head:ident : $typo:ty, $($tail:tt)+) => {
        handle_args!($head: $typo);
        handle_args!($($tail)+);
    };
     () => {
         println!("nünscht wars!");
     };
}

struct SandcrustWrappers {
    bullshit: i32,
}

static  SW: SandcrustWrappers = SandcrustWrappers{bullshit: 0};

macro_rules! wrap_def {

     (fn $f:ident($($x:tt)*) $body:block ) => {
        impl SandcrustWrappers {
            fn $f(&self) {
                println!("invent a method");
        }
        }
         fn $f($($x)*) {
            println!("do something before and then just eat the block");
            SW.$f();
            handle_args!($($x)*);
            $body
         }
     };
}

wrap_def!{
    fn empty() {
         println!("so empty!");
    }
}

wrap_def!{
    fn full(bla: i32, blubb: i64) {
         println!("so full with {} and {}!", bla, blubb);
    }
}

fn main() {
    empty();
    full(32, 1);
}
