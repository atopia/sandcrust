extern crate nix;

extern crate bincode;
extern crate rustc_serialize;

extern crate sandheap;

use std::os::unix::io::FromRawFd;

use sandheap as sandbox;

pub type SandcrustPid = nix::libc::pid_t;
// pub use because of https://github.com/rust-lang/rust/issues/31355
pub use nix::unistd::ForkResult as SandcrustForkResult;

pub struct SandcrustGlobal {
	pub cmd_send: std::os::unix::io::RawFd,
	pub result_receive: std::os::unix::io::RawFd,
	pub child: SandcrustPid,
}

pub static mut SANDCRUST_GLOBAL: SandcrustGlobal = SandcrustGlobal{cmd_send: 0, result_receive: 0, child: 0};
static SANDCRUST_START: ::std::sync::Once = ::std::sync::ONCE_INIT;

// needed as a wrapper for all the imported uses
#[doc(hidden)]
pub struct Sandcrust {
    file_in: ::std::fs::File,
    file_out: ::std::fs::File,
}

impl Sandcrust {
        pub fn new() -> Sandcrust {
            let (fd_out, fd_in) = nix::unistd::pipe().unwrap();
            Sandcrust {
                file_in: unsafe { ::std::fs::File::from_raw_fd(fd_in) },
                file_out: unsafe { ::std::fs::File::from_raw_fd(fd_out) },
            }
	   }

    pub fn setup_sandbox(&self) {
        sandbox::setup();
    }

	fn run_child_loop(&mut self) {
        loop {
            let func_int: u64 = self.restore_var_from_fifo();
            if func_int == 0 {
					::std::process::exit(0);
            } else {
                unsafe {
                   let func: fn(&mut Sandcrust) = std::mem::transmute_copy(&func_int);
                   func(self);
                }
            }
        }
	}

    pub fn new_global() -> Sandcrust {
		// use SANDCRUST_GLOBAL.cmd_send as marker for initialization
        if unsafe {SANDCRUST_GLOBAL.cmd_send == 0} {
            SANDCRUST_START.call_once(|| {
                let (child_cmd_receive, parent_cmd_send ) = ::nix::unistd::pipe().unwrap();
                unsafe { SANDCRUST_GLOBAL.cmd_send = parent_cmd_send};
                let (parent_result_receive, child_result_send ) = ::nix::unistd::pipe().unwrap();
                unsafe { SANDCRUST_GLOBAL.result_receive = parent_result_receive};

			    match ::nix::unistd::fork() {
				    // as parent, simply set SANDCRUST_GLOBAL.child to child PID
				    Ok(::nix::unistd::ForkResult::Parent { child, .. }) => {
					    unsafe { SANDCRUST_GLOBAL.child = child};
				    },
				    // as child, run the IPC loop
				    Ok(::nix::unistd::ForkResult::Child) => {
					    // we overload the meaning of file_in / file_out for parent and child here, which is
					    // not nice but might enable reuse of some methods
					    let mut sandcrust = Sandcrust {
						    file_in: unsafe { ::std::fs::File::from_raw_fd(child_result_send) },
						    file_out: unsafe { ::std::fs::File::from_raw_fd(child_cmd_receive) },
					    };
					    sandcrust.setup_sandbox();
					    sandcrust.run_child_loop();
					    ::std::process::exit(0);
				    }
				    Err(e) => panic!("sandcrust: fork() failed with error {}", e),
			    };
            });
        }
        // dublicate the global raw file descriptors because from_raw_fd will consume them
        // and they will be closed, once the File object goes out of scope
        let new_cmd_send = nix::unistd::dup(unsafe{SANDCRUST_GLOBAL.cmd_send}).unwrap();
        let new_result_receive = nix::unistd::dup(unsafe{SANDCRUST_GLOBAL.result_receive}).unwrap();
        Sandcrust {
            file_in: unsafe { ::std::fs::File::from_raw_fd(new_cmd_send) },
            file_out: unsafe { ::std::fs::File::from_raw_fd(new_result_receive) },
        }
    }

    pub fn join_child(&self, child: SandcrustPid) {
        match nix::sys::wait::waitpid(child, None) {
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

    // wrap fork to avoid exporting nix
    pub fn fork(&self) -> std::result::Result<SandcrustForkResult, nix::Error> {
        nix::unistd::fork()
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

// FIXME: somehow refactor
#[macro_export]
macro_rules! store_vars_fn {
    ($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var_in_fifo($head); };
    ($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)*) => {
        $sandcrust.put_var_in_fifo($head);
        store_vars_fn!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, $head:ident : &$typo:ty) => { };
    ($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
        store_vars_fn!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, $head:ident : $typo:ty ) => { };
    ($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
        store_vars_fn!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, mut $head:ident : $typo:ty ) => { };
    ($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
        store_vars_fn!($sandcrust, $($tail)*);
    };
    ($sandcrust:ident, ) => {};
}

// matching hell, but there is nothing else to do because Push Down Accumulation is a necessity
// https://danielkeep.github.io/tlborm/book/pat-push-down-accumulation.html#incremental-tt-munchers
// unfortunately, using $head:expr seems to match a single macro defition, but fails to expand in a
// subsequent macro
#[macro_export]
macro_rules! strip_types {
    (($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident($($body:expr),+))) => (strip_types!(($($tail)+) -> ($f($($body),+, &mut $head))));
    (($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!(($($tail)+) -> ($f(&mut $head))));
    (($head:ident : &mut $typo:ty) -> ($f:ident($($body:expr),+))) => ($f($($body)+, &mut $head));
    (($head:ident : &mut $typo:ty) -> ($f:ident())) => ($f(&mut $head));

    (($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident($($body:expr),+))) => (strip_types!(($($tail)+) -> ($f($($body),+, &$head))));
    (($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!(($($tail)+) -> ($f(&$head))));
    (($head:ident : &$typo:ty) -> ($f:ident($($body:expr),+))) => ($f($($body)+, &$head));
    (($head:ident : &$typo:ty) -> ($f:ident())) => ($f(&$head));

    ((mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:expr),+))) => (strip_types!(($($tail)+) -> ($f($($body),+, mut $head))));
    ((mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!(($($tail)+) -> ($f(mut $head))));
    ((mut $head:ident : $typo:ty) -> ($f:ident($($body:expr),+))) => ($f($($body)+, mut $head));
    ((mut $head:ident : $typo:ty) -> ($f:ident())) => ($f(mut $head));

    (($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:expr),+))) => (strip_types!(($($tail)+) -> ($f($($body),+, $head))));
    (($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!(($($tail)+) -> ($f($head))));
    (($head:ident : $typo:ty) -> ($f:ident($($body:expr),+))) => ($f($($body)+, $head));
    (($head:ident : $typo:ty) -> ($f:ident())) => ($f($head));

    ($f:ident($($tail:tt)+)) => (strip_types!(($($tail)+) -> ($f())));
    ($f:ident()) => ($f());
}

// FIXME: somehow refactor
#[macro_export]
macro_rules! push_args {
    ($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var_in_fifo(*$head); };
    ($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
        $sandcrust.put_var_in_fifo(*$head);
        push_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : &$typo:ty) => { $sandcrust.put_var_in_fifo($head); };
    ($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
        $sandcrust.put_var_in_fifo($head);
        push_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
        $sandcrust.put_var_in_fifo($head);
        push_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty ) => {
        $sandcrust.put_var_in_fifo($head);
    };
    ($sandcrust:ident, mut $head:ident : $typo:ty ) => { $sandcrust.put_var_in_fifo($head); };
    ($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
        $sandcrust.put_var_in_fifo($head);
        push_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, ) => {};
}

// FIXME: somehow refactor
#[macro_export]
macro_rules! restore_vars_fn {
    ($sandcrust:ident, $head:ident : &mut $typo:ty) => {
        *$head = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
        *$head = $sandcrust.restore_var_from_fifo();
        restore_vars_fn!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : &$typo:ty) => { };
    ($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
        restore_vars_fn!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
        restore_vars_fn!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty ) => { };
    ($sandcrust:ident, mut $head:ident : $typo:ty ) => { };
    ($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
        restore_vars_fn!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, ) => {};
}

// FIXME: somehow refactor
#[macro_export]
macro_rules! pull_args {
    ($sandcrust:ident, $head:ident : &mut $typo:ty) => {
        let mut $head: $typo = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
        let mut $head: $typo = $sandcrust.restore_var_from_fifo();
        pull_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : &$typo:ty) => {
        let $head: &$typo = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
        let $head: &$typo = $sandcrust.restore_var_from_fifo();
        pull_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
        let $head: $typo = $sandcrust.restore_var_from_fifo();
        pull_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty ) => {
        let $head: $typo = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, mut $head:ident : $typo:ty ) => {
        let mut $head: $typo = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
        let mut $head: $typo = $sandcrust.restore_var_from_fifo();
        pull_args!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, ) => {};
}

#[macro_export]
macro_rules! restore_vars {
    // only restore mut types
    ($sandcrust:ident, &mut $head:ident) => {
        $head = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, &mut $head:ident, $($tail:tt)+) => {
        $head = $sandcrust.restore_var_from_fifo();
        restore_vars!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, &$head:ident) => { };
    ($sandcrust:ident, &$head:ident, $($tail:tt)+) => { restore_vars!($sandcrust, $($tail)+); };
    ($sandcrust:ident, $head:ident) => { };
    ($sandcrust:ident, $head:ident, $($tail:tt)+) => { restore_vars!($sandcrust, $($tail)+); };
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
macro_rules! sandbox_internal {
     ($has_retval:ident, $f:ident($($x:tt)*)) => {{
        let mut sandcrust = $crate::Sandcrust::new();
        let child: $crate::SandcrustPid = match sandcrust.fork() {
            Ok($crate::SandcrustForkResult::Parent { child, .. }) => {
                restore_vars!(sandcrust, $($x)*);
                child
            },
            Ok($crate::SandcrustForkResult::Child) => {
                sandcrust.setup_sandbox();
                run_func!($has_retval, sandcrust, $f($($x)*));
                ::std::process::exit(0);
            }
            Err(e) => panic!("sandcrust: fork() failed with error {}", e),
        };
        collect_ret!($has_retval, sandcrust, child)
     }};
}

pub struct SandcrustWrapper;

#[macro_export]
macro_rules! sandbox {
	// retval, potentially args
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(has_ret, $f($($x)*))
     }};

	 // (global-)wrap a function definition, transforming it
     (fn $f:ident($($x:tt)*) $body:block ) => {
         // Fake trait to implement a function to use as a wrapper function.
         // FIXME: ideally this should be done by defining a struct (like SandcrustWrapper) in the macro,
         // but only once (#ifndef bla struct OnlyOnce #define bla #endif - Style) and just adding
         // a method named $f to it - however I haven't been able to figure out how to check for an
         // existing definition.
         // Using a trait instead, because traits can be added to a data type defined (one time) elsewhere.
         // However, the downside is polluting the trait namespace, potentially colliding with
         // existing traits when wrapping functions such as Clone, Drop, etc.
		//  a simple $f_wrapped won't do in any way: https://github.com/rust-lang/rust/issues/12249
         #[allow(non_camel_case_types)]
         trait $f {
             fn $f(sandcrust: &mut $crate::Sandcrust);
         }

	 	 // wrapper function generated to draw the right amount of args from pipe
		 // before calling the whole function client-side
		 // FIXME will likely need to get a sandcrust object anyway, so the question is: is there a way
		 // to make it an impl of sandcrust?
         impl $f for $crate::SandcrustWrapper {
            fn $f(sandcrust: &mut $crate::Sandcrust) {
                println!("look I got magic going!: {}", nix::unistd::getpid());
                pull_args!(sandcrust, $($x)*);
                strip_types!{$f($($x)*)};
                store_vars_fn!(sandcrust, $($x)*);
            }
         }

		 // possibly called by PARENT (and child):
		 // FIXME: am besten gleich: je nach direkt-c oder nicht die in Ruhe lassen und nen anderen
		 // wrapper nehmen
         fn $f($($x)*) {

			// if child is 0 but pipe is set, just run the function, it was called child-side
			 if unsafe { $crate::SANDCRUST_GLOBAL.cmd_send != 0 && $crate::SANDCRUST_GLOBAL.child == 0 } {
			 	 $body
			} else {
					// parent mode, potentially freshly initialized
					println!("parent mode: {}", nix::unistd::getpid());
                    let mut sandcrust = $crate::Sandcrust::new_global();

					// function pointer to newly created method...
                    let func: fn(&mut Sandcrust) = SandcrustWrapper::$f;
                    // ... sent as u64 because this will be serializable
                    // FIXME https://github.com/alexcrichton/cfg-if -> je nach pointer width
                    unsafe {
                       let func_int: u64 = std::mem::transmute(func);
                       sandcrust.put_var_in_fifo(&func_int);
                    }
                    push_args!(sandcrust, $($x)*);
                    restore_vars_fn!(sandcrust, $($x)*);
			}
		}
	};
}

pub fn sandbox_terminate () {
    let mut sandcrust = Sandcrust::new_global();
    sandcrust.put_var_in_fifo(0u64);
    ::nix::unistd::close(unsafe{SANDCRUST_GLOBAL.cmd_send}).unwrap();
    ::nix::unistd::close(unsafe{SANDCRUST_GLOBAL.result_receive}).unwrap();
    sandcrust.join_child(unsafe {SANDCRUST_GLOBAL.child});
}


// no retval
#[macro_export]
macro_rules! sandbox_no_ret {
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(no_ret, $f($($x)*));
     }};
}
