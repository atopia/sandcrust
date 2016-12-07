extern crate nix;

extern crate sandheap;
extern crate memmap;

pub use nix::unistd::{fork, ForkResult};
pub use nix::libc::pid_t;
pub use std::process::exit;

use std::fs::{OpenOptions,remove_file};
use std::mem::size_of;
use nix::sys::wait::{waitpid};
use memmap::{Mmap, Protection};

use sandheap as sandbox;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}


struct Shm{
    file_mmap : Mmap,
}


// FIXME nicer error handling and stuff
impl Shm{
    fn new(size : u64) -> Shm{
        let path: &'static str = "/dev/shm/sandcrust_shm";
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path).unwrap();
        f.set_len(size).unwrap();
        remove_file(path).unwrap();
        Shm {
            file_mmap : Mmap::open(&f, Protection::ReadWrite).unwrap(),
        }
    }

    fn as_ptr(&mut self) -> *mut u8 {
        self.file_mmap.mut_ptr()
    }
}

// needed so that the macro actually finds its shit
#[doc(hidden)]
pub fn sandbox_setup_shm() {
        let size = size_of::<u8>() as u64;
        let mut shm = Shm::new(size);
        let memptr = shm.as_ptr();
        let memref : &mut u8 = unsafe { &mut *memptr };
        println!("memref1 is {}", memref);
        //let middle = unsafe { memptr.offset(2048) };
        *memref = 23;
        println!("memref2 is {}", memref);
}

#[doc(hidden)]
pub fn sandbox_do_parenting(child : pid_t) {
        // FIXME this should be w/ locking
        //*memref = 42;
        match waitpid(child, None) {
            Ok(_) => println!("sandcrust: waitpid() successful"),
            Err(e) => println!("sandcrust waitpid() failed with error {}", e),
        }
        //println!("PARENT: memref is now {}", memref);
}


#[doc(hidden)]
pub fn sandbox_setup_child(){
        sandbox::setup();
        // don't do any of that shit because we don't have the global shm -> create options once
        // that's ready
//        let newptr = shm.as_ptr();
//        let newval : &mut u8 = unsafe { &mut *newptr};
//        println!("CHILD: newval is {}", newval);
//        *newval = 161;
//        println!("CHILD: set newval to {}", newval);
}


#[macro_export]
macro_rules! sandbox_me {
    ($f:ident($($x:expr ),*)) => {{
        sandbox_setup_shm();

        match fork() {
            Ok(ForkResult::Parent { child, .. }) => sandbox_do_parenting(child),
            Ok(ForkResult::Child) => {
                sandbox_setup_child();
                $f($($x),*);
                exit(0);
            }
            Err(e) => println!("sandcrust: fork() failed with error {}", e),
        }
    }}
}
