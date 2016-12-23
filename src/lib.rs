extern crate nix;

extern crate sandheap;
extern crate memmap;

// FIXME still too many exported uses
pub use nix::unistd::{fork, ForkResult};
pub use nix::libc::pid_t;
pub use std::process::exit;
pub use std::mem::size_of_val;

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
    memptr: *mut u8,
}

impl Sandcrust {
    pub fn new(size: usize) -> Sandcrust {
        let size = size as u64;
        Sandcrust { shm: Shm::new(size), memptr: 0 as *mut u8 }
    }

    // FIXME the Method Syntax is the biggest anti-feature in Rust
    pub fn finalize(mut self) -> Sandcrust {
        self.memptr = self.shm.as_ptr();
        self
    }

    pub fn setup_child(&self) {
        sandbox::setup();
    }

    pub fn join_child(&mut self, child: pid_t) {
        match waitpid(child, None) {
            Ok(_) => {},
            Err(e) => println!("sandcrust waitpid() failed with error {}", e),
        }
    }

    pub fn as_ptr(&mut self) -> *mut u8 {
        self.shm.as_ptr()
    }

    pub unsafe fn get_var_in_shm<T>(&mut self, var: T) {
        let size = size_of_val(&var);
        let memptr_orig = self.memptr;
        self.memptr.offset(size as isize);
        let typed_ptr = memptr_orig as *mut T;
        *typed_ptr = var;
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! add_size {
    () => (0);
    ($head:expr) => (size_of_val(&$head));
    ($head:expr, $($tail:expr),*) => (size_of_val(&$head) + add_size!($($tail),*));
}


//
// FIXME hard: refactor!
// - figure out a way to pass 'sandcrust.get_var_in_shm' into the macro
// - is there a way to create more general matchers?
pub fn store_vars_wrapper<T> (sandcrust: &mut Sandcrust, var: T) {
    unsafe {
        sandcrust.get_var_in_shm(var);
    }
}

#[macro_export]
macro_rules! process_vars {
    ($func:ident, &mut $sandcrust:ident, &mut $head:ident) => { $func(&mut $sandcrust, &$head); };
    ($func:ident, &mut $sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        $func(&mut $sandcrust, &$head);
        process_vars!($func, &mut $sandcrust, $($tail)*);
    };
    ($func:ident, &mut $sandcrust:ident, &$head:ident) => { $func(&mut $sandcrust, &$head); };
    ($func:ident, &mut $sandcrust:ident, &$head:ident, $($tail:tt)+) => {
        $func(&mut $sandcrust, &$head);
        process_vars!($func, &mut $sandcrust, $($tail)*);
    };
    ($func:ident, &mut $sandcrust:ident, $head:ident) => { $func(&mut $sandcrust, &$head); };
    ($func:ident, &mut $sandcrust:ident, $head:ident, $($tail:tt)+) => {
        $func(&mut $sandcrust, &$head);
        process_vars!($func, &mut $sandcrust, $($tail)*);
    };
    ($func:ident, &mut $sandcrust:ident, ) => { };
}


#[macro_export]
macro_rules! sandbox_me {
    // FIXME
    // handle no arg and/or ret val cases here
    // and more: use $crate

    // potentially args, no retval
     ($f:ident($($x:tt)*)) => {{
        let mut size:usize = 8;
        size += add_size!($($x)*);
        println!("size is: {}", size);

        let mut sandcrust = Sandcrust::new(size).finalize();
        match fork() {
            Ok(ForkResult::Parent { child, .. }) => {
                sandcrust.join_child(child);
                //reprocess_vars!(sandcrust, $($x)*);
            },
            Ok(ForkResult::Child) => {
                sandcrust.setup_child();
                $f($($x)*);
                process_vars!(store_vars_wrapper, &mut sandcrust, $($x)*);
                exit(0);
            }
            Err(e) => println!("sandcrust: fork() failed with error {}", e),
        }
     }};
}
