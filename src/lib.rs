pub extern crate nix;

extern crate bincode;
extern crate rustc_serialize;

extern crate sandheap;

// this is needed because e.g. fork is exposed in the macro, while the functions from other crates are not
pub use nix as sandcrust_nix;

use std::os::unix::io::FromRawFd;

use sandheap as sandbox;

struct SandcrustGlobal {
	cmd_send: std::os::unix::io::RawFd,
	result_receive: std::os::unix::io::RawFd,
	child: sandcrust_nix::libc::pid_t,
}

static mut SANDCRUST_GLOBAL: SandcrustGlobal = SandcrustGlobal{cmd_send: 0, result_receive: 0, child: 0};

// needed as a wrapper for all the imported uses
#[doc(hidden)]
pub struct Sandcrust {
    file_in: ::std::fs::File,
    file_out: ::std::fs::File,
}

impl Sandcrust {
        pub fn new() -> Sandcrust {
            let (fd_out, fd_in) = sandcrust_nix::unistd::pipe().unwrap();
            Sandcrust {
                file_in: unsafe { ::std::fs::File::from_raw_fd(fd_in) },
                file_out: unsafe { ::std::fs::File::from_raw_fd(fd_out) },
            }
	   }

    pub fn setup_sandbox(&self) {
        sandbox::setup();
    }

	fn run_child_loop(&self) {
		println!("out of the loop!");
	}

    pub fn new_global() -> Sandcrust {
		// use SANDCRUST_PIPE_SEND as marker for initialization
        if unsafe {SANDCRUST_GLOBAL.cmd_send == 0} {
			// FIXME somehow defend against race conditons
            let (child_cmd_receive, parent_cmd_send ) = sandcrust_nix::unistd::pipe().unwrap();
            unsafe { SANDCRUST_GLOBAL.cmd_send = parent_cmd_send};
            let (parent_result_receive, child_result_send ) = sandcrust_nix::unistd::pipe().unwrap();
            unsafe { SANDCRUST_GLOBAL.result_receive = parent_result_receive};

			match sandcrust_nix::unistd::fork() {
				// as parent, simply set SANDCRUST_CHILD_PID
				Ok(sandcrust_nix::unistd::ForkResult::Parent { child, .. }) => {
					unsafe { SANDCRUST_GLOBAL.child = child};
				},
				// as a child, run the IPC loop
				Ok(sandcrust_nix::unistd::ForkResult::Child) => {
					// we overload the meaning of file_in / file_out for parent and child here, which is
					// not nice but might enable reuse of some methods
					let sandcrust = Sandcrust {
						file_in: unsafe { ::std::fs::File::from_raw_fd(child_cmd_receive) },
						file_out: unsafe { ::std::fs::File::from_raw_fd(child_result_send) },
					};
					sandcrust.setup_sandbox();
					sandcrust.run_child_loop();
					::std::process::exit(0);
				}
				Err(e) => panic!("sandcrust: fork() failed with error {}", e),
			};
        }
        Sandcrust {
            file_in: unsafe { ::std::fs::File::from_raw_fd(SANDCRUST_GLOBAL.result_receive) },
            file_out: unsafe { ::std::fs::File::from_raw_fd(SANDCRUST_GLOBAL.cmd_send) },
        }
    }

    pub fn join_child(&self, child: sandcrust_nix::libc::pid_t) {
        match sandcrust_nix::sys::wait::waitpid(child, None) {
            Ok(_) => {}
            Err(e) => println!("sandcrust waitpid() failed with error {}", e),
        }
    }

    pub fn put_var_in_fifo<T: ::rustc_serialize::Encodable>(&mut self, var: T) {
       ::bincode::rustc_serialize::encode_into(&var, &mut self.file_in, ::bincode::SizeLimit::Infinite).unwrap();
    }

    pub fn restore_var_from_fifo<T: ::rustc_serialize::Decodable>(&mut self) -> T {
        ::bincode::rustc_serialize::decode_from(&mut self.file_out, ::bincode::SizeLimit::Infinite).unwrap()
    }

    pub fn terminate_child() {
        unimplemented!();
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
        $head = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
        $head = $sandcrust.restore_var_from_fifo();
        restore_vars!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, &$head:ident) => { };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => { restore_vars!($sandcrust, $($tail)*); };
    ($sandcrust:ident, $head:ident) => { };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => { restore_vars!($sandcrust, $($tail)*); };
    ($sandcrust:ident, ) => {};
}


#[macro_export]
macro_rules! run_func {
     (has_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
       let retval = $f($($x)*);
        store_vars!($sandcrust, $($x)*);
        $sandcrust.put_var_in_fifo(&retval);
     };
     (no_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
       $f($($x)*);
        store_vars!($sandcrust, $($x)*);
    };
}

#[macro_export]
macro_rules! collect_ret {
     (has_ret, $sandcrust:ident, $child:ident) => {{
        let retval = $sandcrust.restore_var_from_fifo();
        $sandcrust.join_child($child);
        retval
     }};
     (no_ret, $sandcrust:ident, $child:ident) => {
        $sandcrust.join_child($child);
     };
}


#[macro_export]
// FIXME: use $crate
macro_rules! sandbox_internal {
     ($has_retval:ident, $f:ident($($x:tt)*)) => {{
        let mut sandcrust = Sandcrust::new();
        let child: sandcrust_nix::libc::pid_t = match sandcrust_nix::unistd::fork() {
            Ok(sandcrust_nix::unistd::ForkResult::Parent { child, .. }) => {
                restore_vars!(sandcrust, $($x)*);
                child
            },
            Ok(sandcrust_nix::unistd::ForkResult::Child) => {
                sandcrust.setup_sandbox();
                run_func!($has_retval, sandcrust, $f($($x)*));
                ::std::process::exit(0);
            }
            Err(e) => panic!("sandcrust: fork() failed with error {}", e),
        };
        collect_ret!($has_retval, sandcrust, child)
     }};
}


// retval, potentially args
#[macro_export]
macro_rules! sandbox {
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(has_ret, $f($($x)*))
     }};
}


// no retval
#[macro_export]
macro_rules! sandbox_no_ret {
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(no_ret, $f($($x)*));
     }};
}
