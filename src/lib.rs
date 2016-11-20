extern crate nix;

extern crate sandheap;

use nix::unistd::{fork, ForkResult};
use sandheap as sandbox;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}


pub fn sandbox_me(func: fn()) {
    match fork() {
        Ok(ForkResult::Parent { child, .. }) => {
            println!("PARENT (child pid {}):", child);
            func();
        }
        Ok(ForkResult::Child) => {
            sandbox::setup();
            println!("CHILD:");
            func();
        }
        Err(e) => println!("Fork failed with error {}", e),
    }
}
