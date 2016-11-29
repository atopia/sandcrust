extern crate nix;

extern crate sandheap;
extern crate memmap;

use nix::unistd::{fork, ForkResult};
use std::fs::{OpenOptions,remove_file};
use std::mem::size_of;
use nix::sys::wait::waitpid;
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


pub fn sandbox_me(func: fn()) {
    let size = size_of::<u8>() as u64;
    let mut shm = Shm::new(size);
    let memptr = shm.as_ptr();
    let memref : &mut u8 = unsafe { &mut *memptr };
    println!("memref1 is {}", memref);
    //let middle = unsafe { memptr.offset(2048) };
    *memref = 23;
    println!("memref2 is {}", memref);

    match fork() {
        Ok(ForkResult::Parent { child, .. }) => {
            // FIXME this should be w/ locking
            *memref = 42;
            println!("PARENT:");
            func();
            match waitpid(child, None) {
                Ok(_) => println!("sandcrust: waitpid() successful"),
                Err(e) => println!("sandcrust waitpid() failed with error {}", e),
            }
        }
        Ok(ForkResult::Child) => {
            sandbox::setup();
            let newptr = shm.as_ptr();
            let newval : &u8 = unsafe { &*newptr};
            println!("newval is {}", newval);
            println!("CHILD:");
            func();
        }
        Err(e) => println!("sandcrust: fork() failed with error {}", e),
    }
}
