extern crate nix;

extern crate sandheap;
extern crate memmap;

// FIXME still too many exported uses
pub use nix::unistd::{fork, ForkResult};
pub use nix::libc::pid_t;
pub use std::process::exit;
pub use std::mem::size_of_val;
pub use std::mem::transmute;
pub use std::mem::transmute_copy;

use std::fs::{OpenOptions, remove_file};
use nix::sys::wait::waitpid;
use nix::unistd::gettid;
use memmap::{Mmap, Protection};

use sandheap as sandbox;


struct Shm {
    file_mmap: Mmap,
}


// FIXME nicer error handling and stuff
impl Shm {
    fn new(size: u64) -> Shm {
        // FIXME any nicer way to do this?
        let basepath = "/dev/shm/sandcrust_shm_".to_string();
        let pid_string = gettid().to_string();
        let path = basepath + &pid_string;
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();
        f.set_len(size).unwrap();
        remove_file(&path).unwrap();
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
        // FIXME: transmute really necessary?
        let typed_ref: &mut T = transmute(&mut *memptr_orig);
        *typed_ref = var;
    }

    pub unsafe fn move_memptr<T>(&mut self, var: &T) {
        let size = size_of_val(var);
        self.memptr.offset(size as isize);
    }

    pub unsafe fn restore_var_from_shm<T>(&self, var: &mut T) {
        let size = size_of_val(var);
        *var = transmute_copy(&*self.memptr);
        self.memptr.offset(size as isize);
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! add_size {
    (&mut $head:ident) => (size_of_val(&$head));
    (&mut $head:ident, $($tail:tt)+) => (size_of_val(&$head) + add_size!($($tail)+));
    (&$head:ident) => (size_of_val(&$head));
    (&$head:ident, $($tail:tt)+) => (size_of_val(&$head) + add_size!($($tail)+));
    ($head:ident) => (size_of_val(&$head));
    ($head:ident, $($tail:tt)+) => (size_of_val(&$head) + add_size!($($tail)+));
    () => (0);
}


// FIXME: somehow refactor
#[macro_export]
macro_rules! store_vars {
    ($sandcrust:ident, &mut $head:ident) => {unsafe {$sandcrust.get_var_in_shm(&$head);};};
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        unsafe {$sandcrust.get_var_in_shm(&$head);};
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, &$head:ident) => { unsafe {$sandcrust.get_var_in_shm(&$head);}; };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => {
        unsafe {$sandcrust.get_var_in_shm(&$head);};
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, $head:ident) => { unsafe {$sandcrust.get_var_in_shm(&$head);}; };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => {
        unsafe {$sandcrust.get_var_in_shm(&$head);};
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, ) => {};
}


#[macro_export]
macro_rules! restore_vars {
    // only restore mut types
    ($sandcrust:ident, &mut $head:ident) => {unsafe {$sandcrust.restore_var_from_shm(&mut $head);};};
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        unsafe {$sandcrust.restore_var_from_shm(&mut $head);};
        restore_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, &$head:ident) => { unsafe {$sandcrust.move_memptr(&$head);}; };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => {
        unsafe {$sandcrust.move_memptr(&$head);};
        restore_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, $head:ident) => { unsafe {$sandcrust.move_memptr(&$head);}; };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => {
        unsafe {$sandcrust.move_memptr(&$head);};
        restore_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, ) => {};
}


#[macro_export]
macro_rules! sandbox_me {
    // FIXME
    // handle no arg and/or ret val cases here
    // and more: use $crate

    // potentially args, no retval
     ($f:ident($($x:tt)*)) => {{
         // FIXME 0
        let mut size:usize = 8;
        size += add_size!($($x)*);

        let mut sandcrust = Sandcrust::new(size).finalize();
        match fork() {
            Ok(ForkResult::Parent { child, .. }) => {
                sandcrust.join_child(child);
                restore_vars!(sandcrust, $($x)*);
            },
            Ok(ForkResult::Child) => {
                sandcrust.setup_child();
                $f($($x)*);
                store_vars!(sandcrust, $($x)*);
                exit(0);
            }
            Err(e) => println!("sandcrust: fork() failed with error {}", e),
        }
     }};
}


#[cfg(test)]
mod internal_tests {
    use super::*;

    #[test]
    fn calc_ref_u8_size() {
        let x: u8 = 8;
        assert!(add_size!(&x) == 1);
    }

    #[test]
    fn calc_ref_mut_u8_size() {
        let mut x:u8 = 8;
        assert!(add_size!(&mut x) == 1);
        x += 1;
        if x < 8 {
        }
    }

    #[test]
    fn calc_u8_size() {
        let x: u8 = 8;
        assert!(add_size!(x) == 1);
    }

    #[test]
    fn calc_i32_size() {
        let x: i32 = 23;
        assert!(add_size!(x) == 4);
    }

    #[test]
    fn calc_size_multi() {
        let x: i32 = 23;
        let y: u8 = 23;
        assert!(add_size!(x, y) == 5);
    }

    #[test]
    fn calc_size_ref_multi() {
        let x: i32 = 23;
        let y: u8 = 23;
        let mut z: u64 = 23;
        assert!(add_size!(x, &y, &mut z) == 13);
        z += 1;
        if z < 8 {
        }
    }
}
