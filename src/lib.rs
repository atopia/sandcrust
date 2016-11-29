extern crate nix;

extern crate sandheap;
extern crate memmap;

use nix::unistd::{fork, ForkResult};
use std::fs::File;
//use nix::sys::mman::*;
use sandheap as sandbox;
use nix::sys::wait::waitpid;
use memmap::{Mmap, Protection};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}


//struct Shm<'ptr>{
struct Shm{
    file_mmap : Mmap,
    //buf : &'ptr [u8],
}

//impl<'ptr> Shm<'ptr>{
impl Shm{
    //fn new(size : u64) -> Shm<'ptr>{
    fn new(size : u64) -> Shm{
        // FIXME nicer error handling and stuff
        let f = File::create("/dev/shm/sandcrust_shm").unwrap();
        f.set_len(size).unwrap();
       // let file_mmap = Mmap::open(&f, Protection::Read).unwrap();
        //let buf = unsafe { file_mmap.as_slice() };
        Shm {
            file_mmap : Mmap::open(&f, Protection::Read).unwrap(),
            //file_mmap : file_mmap,
            //buf : unsafe { file_mmap.as_slice() },
            //buf : buf,
        }
    }

    fn get_ptr(&self) -> *const u8 {
        self.file_mmap.ptr()
    }
}


pub fn sandbox_me(func: fn()) {
    let shm = Shm::new(2);
    let memptr = shm.get_ptr();
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
