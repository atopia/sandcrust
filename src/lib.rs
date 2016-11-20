extern crate nix;

extern crate sandheap;

use nix::unistd::{fork, ForkResult};
use sandheap as sandbox;
use nix::sys::wait::waitpid;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
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
