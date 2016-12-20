extern crate sandcrust;

extern crate nix;
extern crate libc;
extern crate errno;

use nix::unistd::getpid;
use libc::{readlink, c_char};
use std::ffi::CString;
use errno::errno;

use sandcrust::*;


fn get_mnt_ns() {
    let pid = getpid();
    // FIXME some nicer way to build a path?
    let pathstr = "/proc/".to_string() + &pid.to_string() + "/ns/mnt";
    let path = CString::new(pathstr).unwrap();

    // jeez this is ugly as fuck
    let mut x: Vec<c_char> = vec![0; 256];
    let slice = x.as_mut_slice();
    let bufptr = slice.as_mut_ptr();

    unsafe {
        if readlink(path.as_ptr(), bufptr, 255) > 0 {
            let contents = CString::from_raw(bufptr).into_string().unwrap();
            println!("mnt ns: {}", contents);
        } else {
            let e = errno();
            println!("read failed: {}", e);
        }
    }
}


fn write_b_to_a(a : &mut i32, b : &mut i32) {
    *a = *b;
    println!("a is now: {}", a);
}

fn eat_a_b(a: i32, mut b: i32) {
    if b > a {
        b = a;
    }
    println!("b is now: {}", b);
}

fn empty() {
    println!("this function args is none");
}

fn ref_to_a(a: &i32) {
    println!("this function is passed a ref to {}", a);
}

fn take_a(a: i32) {
    println!("this function is passed {}", a);
}

pub fn main() {
    println!("PARENT: now sandboxing child");
    sandbox_me!(get_mnt_ns());

    println!("PARENT:");
    get_mnt_ns();

    let mut a = 23;
    let mut b = 42;
    println!("b was: {}", b);
    sandbox_me!(empty());
    sandbox_me!(take_a(a));
    sandbox_me!(ref_to_a(&a));
    sandbox_me!(ref_to_a(&mut b));
    sandbox_me!(write_b_to_a(&mut a, &mut b));
    sandbox_me!(eat_a_b(a, b));
    println!("b is now: {}", b);
}
