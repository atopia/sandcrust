pub extern crate nix;

extern crate bincode;
extern crate rustc_serialize;
extern crate libc;
extern crate errno;

extern crate sandheap;

pub use nix as sandcrust_nix;

// FIXME make absolute
use bincode::SizeLimit;
use bincode::rustc_serialize::{encode_into, decode_from};
use rustc_serialize::{Encodable, Decodable};

use sandheap as sandbox;

// needed as a wrapper for all the imported uses
#[doc(hidden)]
pub struct Sandcrust {
    fifo_path: String
}

impl Sandcrust {
        pub fn new() -> Sandcrust {
        let basepath = "/tmp/sandcrust_pipe_".to_string();
        let pid_string = sandcrust_nix::unistd::gettid().to_string();
        let path = basepath + &pid_string;
	    let cpathstr = ::std::ffi::CString::new(path.clone()).unwrap();
        unsafe {
	        let cpath = cpathstr.as_ptr();
            let ret = ::libc::mkfifo(cpath, 0o666);
            if ret != 0 {
                let e = ::errno::errno();
                panic!("FIFO creation failed with error {}", e);
            }
        }
        Sandcrust {
            fifo_path: path,
        }
    }

    pub fn setup_child(&self) {
        sandbox::setup();
    }

    pub fn join_child(&self, child: sandcrust_nix::libc::pid_t) {
        match sandcrust_nix::sys::wait::waitpid(child, None) {
            Ok(_) => {}
            Err(e) => println!("sandcrust waitpid() failed with error {}", e),
        }
        ::std::fs::remove_file(&self.fifo_path).unwrap();
    }

    pub fn put_var_in_fifo<T: Encodable>(&self, var: T) {
        // extra scope to close the file early
        {
            let mut fifo = ::std::fs::OpenOptions::new()
                .write(true)
                .open(&self.fifo_path)
                .unwrap();
            encode_into(&var, &mut fifo, SizeLimit::Infinite).unwrap();
        }
    }

    pub fn restore_var_from_fifo<T: Decodable>(&self, var: &mut T) {
        let mut fifo = ::std::fs::File::open(&self.fifo_path).unwrap();
        *var = decode_from(&mut fifo, SizeLimit::Infinite).unwrap();
    }
}


// FIXME: somehow refactor
#[macro_export]
macro_rules! store_vars {
    ($sandcrust:ident, &mut $head:ident) => { $sandcrust.put_var_in_fifo($head); };
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        $sandcrust.put_var_in_fifo($head);
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, &$head:ident) => { };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => {
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, $head:ident) => { };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => {
        store_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, ) => {};
}


#[macro_export]
macro_rules! restore_vars {
    // only restore mut types
    ($sandcrust:ident, &mut $head:ident) => {
        $sandcrust.restore_var_from_fifo(&mut $head);
    };
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        $sandcrust.restore_var_from_fifo(&mut $head);
        restore_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, &$head:ident) => { };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => { restore_vars!($sandcrust, $($tail)*); };
    ($sandcrust:ident, $head:ident) => { };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => { restore_vars!($sandcrust, $($tail)*); };
    ($sandcrust:ident, ) => {};
}


#[macro_export]
macro_rules! sandbox_me {
    // FIXME
    // handle no arg and/or ret val cases here
    // and more: use $crate

    // potentially args, no retval
     ($f:ident($($x:tt)*)) => {{
        let mut sandcrust = Sandcrust::new();
        match sandcrust_nix::unistd::fork() {
            Ok(sandcrust_nix::unistd::ForkResult::Parent { child, .. }) => {
                restore_vars!(sandcrust, $($x)*);
                sandcrust.join_child(child);
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
