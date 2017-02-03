extern crate nix;

extern crate bincode;
extern crate rustc_serialize;

extern crate sandheap;

#[macro_use]
extern crate lazy_static;

use std::os::unix::io::FromRawFd;

use sandheap as sandbox;

pub type SandcrustPid = nix::libc::pid_t;
// pub use because of https://github.com/rust-lang/rust/issues/31355
pub use nix::unistd::ForkResult as SandcrustForkResult;

// needed as a wrapper for all the imported uses
#[doc(hidden)]
pub struct Sandcrust {
    file_in: ::std::fs::File,
    file_out: ::std::fs::File,
    child: SandcrustPid,
}

lazy_static! {
    pub static ref SANDCRUST: ::std::sync::Arc<::std::sync::Mutex<Sandcrust>> = { std::sync::Arc::new(std::sync::Mutex::new(Sandcrust::new_global())) };
}

// necessary, because once the child is initialized, we need a lightweight, non-locking check to
// run the original function
// changing this is protected by SANDCRUST's mutex
pub static mut INITIALIZED_CHILD: bool = false;

impl Sandcrust {
        pub fn new() -> Sandcrust {
            let (fd_out, fd_in) = nix::unistd::pipe().unwrap();
            Sandcrust {
                file_in: unsafe { ::std::fs::File::from_raw_fd(fd_in) },
                file_out: unsafe { ::std::fs::File::from_raw_fd(fd_out) },
                child: 0,
            }
	   }

    // if we're the child, but not yet initialized, run child loop
    pub fn initialize_child(&mut self) {
        if ! unsafe{INITIALIZED_CHILD} && self.child == 0 {
            unsafe{INITIALIZED_CHILD = true};
			self.run_child_loop();
        }
    }

    pub fn setup_sandbox(&self) {
        sandbox::setup();
    }

	fn run_child_loop(&mut self) {
        self.setup_sandbox();
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
        let (child_cmd_receive, parent_cmd_send ) = ::nix::unistd::pipe().unwrap();
        let (parent_result_receive, child_result_send ) = ::nix::unistd::pipe().unwrap();

		let mut sandcrust = match ::nix::unistd::fork() {
			Ok(::nix::unistd::ForkResult::Parent { child, .. }) => {
                Sandcrust {
                    file_in: unsafe { ::std::fs::File::from_raw_fd(parent_cmd_send) },
                    file_out: unsafe { ::std::fs::File::from_raw_fd(parent_result_receive) },
                    child: child,
                }
			},
			Ok(::nix::unistd::ForkResult::Child) => {
				// we overload the meaning of file_in / file_out for parent and child here, which is
				// not nice but might enable reuse of some methods
				Sandcrust {
					file_in: unsafe { ::std::fs::File::from_raw_fd(child_result_send) },
					file_out: unsafe { ::std::fs::File::from_raw_fd(child_cmd_receive) },
                    child: 0,
				}
			}
			Err(e) => panic!("sandcrust: fork() failed with error {}", e),
		};
        sandcrust
    }

    pub fn join_child(&self) {
        match nix::sys::wait::waitpid(self.child, None) {
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

    pub fn terminate_child(&mut self) {
        self.put_var_in_fifo(0u64);
        self.join_child();
    }

    pub fn set_child(&mut self, child: SandcrustPid) {
        self.child = child;
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
    ($sandcrust:ident, mut $head:ident : $typo:ty ) => { };
    ($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
        restore_vars_fn!($sandcrust, $($tail)+);
    };
    ($sandcrust:ident, $head:ident : $typo:ty ) => { };
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
        let $head: $typo = $sandcrust.restore_var_from_fifo();
    };
    ($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
        let $head: $typo = $sandcrust.restore_var_from_fifo();
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
     (has_ret, $sandcrust:ident) => {{
        let retval = $sandcrust.restore_var_from_fifo();
        $sandcrust.join_child();
        retval
     }};
     (no_ret, $sandcrust:ident) => {
        $sandcrust.join_child();
     };
}


// matching hell, but there is nothing else to do because Push Down Accumulation is a necessity
// https://danielkeep.github.io/tlborm/book/pat-push-down-accumulation.html#incremental-tt-munchers
// unfortunately, using $head:expr seems to match a single macro defition, but fails to expand in a
// subsequent macro
#[macro_export]
macro_rules! strip_types {
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, &mut $head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f(&mut $head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, &mut $head)));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f(&mut $head)));

    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, &$head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f(&$head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, &$head)));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f(&$head)));

    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, mut $head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f(mut $head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, $head)));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f($head)));

    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, $head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($head))));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, $head)));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f($head)));

    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, $f:ident($($tail:tt)+)) => (strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f())));
    ($called_macro:ident, $has_retval:ident, $sandcrust:ident, $f:ident()) => ($called_macro!($has_retval, $sandcrust, $f()));
}



#[macro_export]
macro_rules! sandbox_internal {
     ($has_retval:ident, $f:ident($($x:tt)*)) => {{
        let mut sandcrust = $crate::Sandcrust::new();
        match sandcrust.fork() {
            Ok($crate::SandcrustForkResult::Parent { child, .. }) => {
                restore_vars!(sandcrust, $($x)*);
                sandcrust.set_child(child);
            },
            Ok($crate::SandcrustForkResult::Child) => {
                sandcrust.setup_sandbox();
                run_func!($has_retval, sandcrust, $f($($x)*));
                ::std::process::exit(0);
            }
            Err(e) => panic!("sandcrust: fork() failed with error {}", e),
        };
        collect_ret!($has_retval, sandcrust)
     }};
}

pub struct SandcrustWrapper;

#[macro_export]
macro_rules! collect_ret_global {
     (has_ret, $rettype:ty, $sandcrust:ident) => {{
        let retval: $rettype = $sandcrust.restore_var_from_fifo();
        retval
     }};
     (no_ret, $rettype:ty, $sandcrust:ident) => { () };
}


#[macro_export]
macro_rules! sandbox_global_create_wrapper {
    ($has_retval:ident, fn $f:ident($($x:tt)*)) => {
         // Fake trait to implement a function to use as a wrapper function.
         // FIXME: ideally this should be done by defining a struct (like SandcrustWrapper) in the macro,
         // but only once (#ifndef bla struct OnlyOnce; #define bla #endif - Style) and just adding
         // a method named $f to it - however I haven't been able to figure out how to check for an
         // existing definition at compile time.
         // Using a trait instead, because traits can be added to a data type defined (once) elsewhere.
         // However, the downside is polluting the trait namespace, potentially colliding with
         // existing traits when wrapping functions such as Clone, Drop, etc.
		//  a simple $f_wrapped won't do in any way: https://github.com/rust-lang/rust/issues/12249
         #[allow(non_camel_case_types)]
         trait $f {
             fn $f(sandcrust: &mut $crate::Sandcrust);
         }

	 	 // wrapper function generated to draw the right amount of args from pipe
		 // before calling the whole function client-side
         // It would be awesome to bind this to the existing struct Sandcrust, however at the
         // expense of possible function name collisions.
         impl $f for $crate::SandcrustWrapper {
            fn $f(sandcrust: &mut $crate::Sandcrust) {
                //println!("look I got magic going!: {}", ::nix::unistd::getpid());
                pull_args!(sandcrust, $($x)*);
                strip_types!(run_func, $has_retval, sandcrust, $f($($x)*));
            }
         }
    };
}

// possibly called by PARENT (and child):
// FIXME: am besten gleich: je nach direkt-c oder nicht die in Ruhe lassen und nen anderen
// wrapper nehmen
#[macro_export]
macro_rules! sandbox_global_create_function {
    ($has_retval:ident, fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
			// as an initialized child, just run function
			if unsafe{INITIALIZED_CHILD} {
				 $body
			} else {
                    let mut sandcrust = SANDCRUST.lock().unwrap();
                    // potenially completely unintialized, if we're the child on first access, run
                    // child loop
                    sandcrust.initialize_child();

					// parent mode, potentially freshly initialized
					//println!("parent mode: {}", ::nix::unistd::getpid());

					// function pointer to newly created method...
                    let func: fn(&mut $crate::Sandcrust) = $crate::SandcrustWrapper::$f;
                    // ... sent as u64 because this will be serializable
                    // FIXME use if cfg!(target_pointer_width = "32"), but seems broken
                    unsafe {
                       let func_int: u64 = ::std::mem::transmute(func);
                       sandcrust.put_var_in_fifo(&func_int);
                    }
                    push_args!(sandcrust, $($x)*);
                    restore_vars_fn!(sandcrust, $($x)*);
                    collect_ret_global!($has_retval, $rettype, sandcrust)
            }
    };
}


#[macro_export]
macro_rules! sandbox {
	// retval, potentially args
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(has_ret, $f($($x)*))
     }};
	 // (global-)wrap a function definition, transforming it
     (pub fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
        sandbox_global_create_wrapper!(has_ret, fn $f($($x)*));
         pub fn $f($($x)*) -> $rettype {
            sandbox_global_create_function!(has_ret, fn $f($($x)*) -> $rettype $body)
		}
	 };
     (pub fn $f:ident($($x:tt)*) $body:block ) => {
        sandbox_global_create_wrapper!(no_ret, fn $f($($x)*));
         pub fn $f($($x)*) {
            sandbox_global_create_function!(no_ret, fn $f($($x)*) -> i32 $body)
		}
	};
     (fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
        sandbox_global_create_wrapper!(has_ret, fn $f($($x)*));
         fn $f($($x)*) -> $rettype {
            sandbox_global_create_function!(has_ret, fn $f($($x)*) -> $rettype $body)
		}
	 };
     (fn $f:ident($($x:tt)*) $body:block ) => {
        sandbox_global_create_wrapper!(no_ret, fn $f($($x)*));
         fn $f($($x)*) {
            sandbox_global_create_function!(no_ret, fn $f($($x)*) -> i32 $body)
		}
	};
}

pub fn sandbox_terminate () {
    let mut sandcrust = SANDCRUST.lock().unwrap();
    sandcrust.terminate_child();
}


// no retval
#[macro_export]
macro_rules! sandbox_no_ret {
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(no_ret, $f($($x)*));
     }};
}
