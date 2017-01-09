extern crate memmap;
pub extern crate nix;

extern crate sandheap;

pub use nix as sandcrust_nix;
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
        let pid_string = sandcrust_nix::unistd::gettid().to_string();
        let path = basepath + &pid_string;
        let f = ::std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();
        f.set_len(size).unwrap();
        ::std::fs::remove_file(&path).unwrap();
        Shm { file_mmap: Mmap::open(&f, Protection::ReadWrite).unwrap() }
    }

    fn as_ptr(&mut self) -> *mut u8 {
        self.file_mmap.mut_ptr()
    }
}


#[doc(hidden)]
#[macro_export]
macro_rules! add_size {
    (&mut $head:ident) => (::std::mem::size_of_val(&$head));
    (&mut $head:ident, $($tail:tt)+) => (::std::mem::size_of_val(&$head) + add_size!($($tail)+));
    (&$head:ident) => (::std::mem::size_of_val(&$head));
    (&$head:ident, $($tail:tt)+) => (::std::mem::size_of_val(&$head) + add_size!($($tail)+));
    ($head:ident) => (::std::mem::size_of_val(&$head));
    ($head:ident, $($tail:tt)+) => (::std::mem::size_of_val(&$head) + add_size!($($tail)+));
    () => (0);
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
        Sandcrust {
            shm: Shm::new(size),
            memptr: 0 as *mut u8,
        }
    }

    // FIXME the Method Syntax is the biggest anti-feature in Rust
    pub fn finalize(mut self) -> Sandcrust {
        self.memptr = self.shm.as_ptr();
        self
    }

    pub fn setup_child(&self) {
        sandbox::setup();
    }

    pub fn join_child(&mut self, child: sandcrust_nix::libc::pid_t) {
        match sandcrust_nix::sys::wait::waitpid(child, None) {
            Ok(_) => {}
            Err(e) => println!("sandcrust waitpid() failed with error {}", e),
        }
    }

    pub fn as_ptr(&mut self) -> *mut u8 {
        self.shm.as_ptr()
    }

    pub unsafe fn get_var_in_shm<T>(&mut self, var: T) {
        let size = add_size!(var);
        // FIXME: ::std::mem::transmute really necessary?
        let typed_ref: &mut T = ::std::mem::transmute(&mut *self.memptr);
        *typed_ref = var;
        self.memptr = self.memptr.offset(size as isize);
    }

    pub unsafe fn move_memptr<T>(&mut self, var: &T) {
        // FIXME add_size wouldn't catch a double deref, so for now this
        let size = ::std::mem::size_of_val(&*var);
        self.memptr = self.memptr.offset(size as isize);
    }

    pub unsafe fn restore_var_from_shm<T>(&mut self, var: &mut T) {
        // FIXME add_size wouldn't catch a double deref, so for now this
        let size = ::std::mem::size_of_val(&*var);
        *var = ::std::mem::transmute_copy(&*self.memptr);
        self.memptr = self.memptr.offset(size as isize);
    }
}


// FIXME: somehow refactor
#[macro_export]
macro_rules! store_vars {
    ($sandcrust:ident, &mut $head:ident) => {unsafe {$sandcrust.get_var_in_shm($head);};};
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        unsafe {$sandcrust.get_var_in_shm($head);};
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, &$head:ident) => { unsafe {$sandcrust.get_var_in_shm($head);}; };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => {
        unsafe {$sandcrust.get_var_in_shm($head);};
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, $head:ident) => { unsafe {$sandcrust.get_var_in_shm($head);}; };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => {
        unsafe {$sandcrust.get_var_in_shm($head);};
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, ) => {};
}


#[macro_export]
macro_rules! restore_vars {
    // only restore mut types
    ($sandcrust:ident, &mut $head:ident) => {
        unsafe {$sandcrust.restore_var_from_shm(&mut $head);};
    };
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
        match sandcrust_nix::unistd::fork() {
            Ok(sandcrust_nix::unistd::ForkResult::Parent { child, .. }) => {
                sandcrust.join_child(child);
                restore_vars!(sandcrust, $($x)*);
            },
            Ok(sandcrust_nix::unistd::ForkResult::Child) => {
                sandcrust.setup_child();
                $f($($x)*);
                store_vars!(sandcrust, $($x)*);
                ::std::process::exit(0);
            }
            Err(e) => println!("sandcrust: fork() failed with error {}", e),
        }
     }};
}


#[cfg(test)]
mod internal_tests {

    #[test]
    fn calc_ref_u8_size() {
        let x: u8 = 8;
        assert!(add_size!(&x) == 1);
    }

    #[test]
    fn calc_ref_mut_u8_size() {
        let mut x: u8 = 8;
        assert!(add_size!(&mut x) == 1);
        x += 1;
        if x < 8 {
        }
    }

    #[test]
    // FIXME
    #[ignore]
    fn calc_double_ref_u8_size() {
        let x: u8 = 8;
        let y = &x;
        assert!(add_size!(&y) == 1);
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
