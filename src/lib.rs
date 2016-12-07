extern crate nix;

extern crate sandheap;
extern crate memmap;

// FIXME still too many exported uses
pub use nix::unistd::{fork, ForkResult};
pub use nix::libc::pid_t;
pub use std::process::exit;
// FIXME demo code
pub use std::mem::size_of;

use std::fs::{OpenOptions, remove_file};
use nix::sys::wait::waitpid;
use memmap::{Mmap, Protection};

use sandheap as sandbox;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}


struct Shm {
    file_mmap: Mmap,
}


// FIXME nicer error handling and stuff
impl Shm {
    fn new(size: u64) -> Shm {
        let path: &'static str = "/dev/shm/sandcrust_shm";
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .unwrap();
        f.set_len(size).unwrap();
        remove_file(path).unwrap();
        Shm { file_mmap: Mmap::open(&f, Protection::ReadWrite).unwrap() }
    }

    fn as_ptr(&mut self) -> *mut u8 {
        self.file_mmap.mut_ptr()
    }
}


// needed as a wrapper for all the imported uses
#[doc(hidden)]
pub struct Sandcrust {
    shm: Shm,
}

impl Sandcrust {
    pub fn new(size: usize) -> Sandcrust {
        let size = size as u64;
        Sandcrust { shm: Shm::new(size) }
    }

    pub fn setup_child(&mut self) {
        sandbox::setup();

        // FIXME demo code
        let newptr = self.shm.as_ptr();
        let newval: &mut u8 = unsafe { &mut *newptr };
        println!("CHILD: newval is {}", newval);
        *newval = 161;
        println!("CHILD: set newval to {}", newval);
    }

    pub fn join_child(&mut self, child: pid_t) {
        match waitpid(child, None) {
            Ok(_) => println!("sandcrust: waitpid() successful"),
            Err(e) => println!("sandcrust waitpid() failed with error {}", e),
        }

        // FIXME demo code
        let memptr = self.shm.as_ptr();
        let memref: &mut u8 = unsafe { &mut *memptr };
        println!("PARENT: memref is now {}", memref);
    }
}


#[macro_export]
macro_rules! sandbox_me {
    ($f:ident($($x:expr ),*)) => {{
        let mut sandcrust = Sandcrust::new(size_of::<u8>());

        match fork() {
            Ok(ForkResult::Parent { child, .. }) => sandcrust.join_child(child),
            Ok(ForkResult::Child) => {
                sandcrust.setup_child();
                $f($($x),*);
                exit(0);
            }
            Err(e) => println!("sandcrust: fork() failed with error {}", e),
        }
    }}
}
