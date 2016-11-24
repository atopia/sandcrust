extern crate nix;

extern crate sandheap;

use nix::unistd::{unlink, mkstemp, lseek, Whence, write, fork, ForkResult};
use nix::sys::mman::*;
use nix::c_void;
use sandheap as sandbox;
use nix::sys::wait::waitpid;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}


fn setup_shm(size : i64) {
    // FIXME nicer error handling and stuff
    let fd = match mkstemp("/dev/shm/sandcrust_shm_XXXXXX") {
        Ok((fd, path)) => {
            unlink(path.as_path()).unwrap(); // flag file to be deleted at app termination
            fd
        }
        Err(e) => panic!("mkstemp failed: {}", e)
    };
    lseek(fd, size - 1, Whence::SeekSet).unwrap();
    // write a single byte at size -1 to stretch the file to size
    write(fd, &[0u8]).unwrap();
    let ptr = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0).unwrap();
}


pub fn sandbox_me(func: fn()) {
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
