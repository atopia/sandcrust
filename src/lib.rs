extern crate nix;
extern crate libc;
extern crate errno;

extern crate sandheap;

use libc::{readlink, c_char};
use std::ffi::CString;
use errno::errno;

use nix::unistd::{fork, ForkResult, getpid};
use sandheap as sandbox;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}

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

pub fn sandbox_me() {
    match fork() {
        Ok(ForkResult::Parent { child, .. }) => {
            println!("PARENT (child pid {}):", child);
            get_mnt_ns();
        }
        Ok(ForkResult::Child) => {
            sandbox::setup();
            println!("CHILD:");
            get_mnt_ns();
        }
        Err(e) => println!("Fork failed with error {}", e),
    }
}
