extern crate nix;

use nix::unistd::{fork, ForkResult};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

pub fn sandbox_me() {
    match fork() {
        Ok(ForkResult::Parent { child, .. }) => {
            println!("parent: new child has pid: {}", child);
        }
        Ok(ForkResult::Child) => println!("child: miau"),
        Err(e) => println!("Fork failed with error {}", e),
    }
}
