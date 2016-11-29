extern crate nix;

extern crate sandheap;
extern crate memmap;

use nix::unistd::{fork, ForkResult};
use std::fs::{OpenOptions,remove_file};
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

impl Shm{
    fn new(size : u64) -> Shm{
        let path: &'static str = "/dev/shm/sandcrust_shm";
        // FIXME nicer error handling and stuff
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path).unwrap();
        f.set_len(size).unwrap();
        remove_file(path).unwrap();
        Shm {
            file_mmap : Mmap::open(&f, Protection::Read).unwrap(),
        }
    }

    fn as_ptr(&self) -> *const u8 {
        self.file_mmap.ptr()
    }
}


pub fn sandbox_me(func: fn()) {
    let shm = Shm::new(4096);
    let memptr = shm.as_ptr();
    let middle = unsafe { memptr.offset(2048) };
    let points_at = unsafe { *middle };
    println!("shm points at {}", points_at);

    match fork() {
        Ok(ForkResult::Parent { child, .. }) => {
            println!("PARENT:");
            func();
            match waitpid(child, None) {
                Ok(_) => println!("sandcrust: waitpid() successful"),
                Err(e) => println!("sandcrust waitpid() failed with error {}", e),
            }
        }
        Ok(ForkResult::Child) => {
            sandbox::setup();
            println!("CHILD:");
            func();
        }
        Err(e) => println!("sandcrust: fork() failed with error {}", e),
    }
}
