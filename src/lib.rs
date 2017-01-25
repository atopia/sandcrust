extern crate nix;

extern crate bincode;
extern crate rustc_serialize;

extern crate sandheap;

use std::os::unix::io::FromRawFd;

use sandheap as sandbox;

pub type SandcrustPid = nix::libc::pid_t;
// pub use because of https://github.com/rust-lang/rust/issues/31355
pub use nix::unistd::ForkResult as SandcrustForkResult;

struct SandcrustGlobal {
	cmd_send: std::os::unix::io::RawFd,
	result_receive: std::os::unix::io::RawFd,
	child: SandcrustPid,
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
            let (fd_out, fd_in) = nix::unistd::pipe().unwrap();
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
            let (child_cmd_receive, parent_cmd_send ) = ::nix::unistd::pipe().unwrap();
            unsafe { SANDCRUST_GLOBAL.cmd_send = parent_cmd_send};
            let (parent_result_receive, child_result_send ) = ::nix::unistd::pipe().unwrap();
            unsafe { SANDCRUST_GLOBAL.result_receive = parent_result_receive};

			match ::nix::unistd::fork() {
				// as parent, simply set SANDCRUST_CHILD_PID
				Ok(::nix::unistd::ForkResult::Parent { child, .. }) => {
					unsafe { SANDCRUST_GLOBAL.child = child};
				},
				// as a child, run the IPC loop
				Ok(::nix::unistd::ForkResult::Child) => {
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


#[macro_export]
macro_rules! sandbox {
	// retval, potentially args
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(has_ret, $f($($x)*))
     }};

	 // (global-)wrap a function definition, transforming it
     (fn $f:ident($($x:tt)*) $body:block ) => {
	 	 // wrapper function generated to draw the right amount of args from pipe
		 // before calling the whole function client-side
		 // will likely need to get a sandcrust object anyway, so the question is: is there a way
		 // to make it an impl of sandcrust?

        // https://github.com/rust-lang/rust/issues/12249
        //  a simple $f_wrapped won't do in any way, therefore:
	 	 fn wrap_$f() {
			 println!("implement wrapper");

			//  restore_vars_from parent->child pipe, using type information (this should work
			//  okay via shadowing lets instead of overwrites)
			 // get_args_with_type!($($x)*);
			 // let a: type = decode...
			 // magic macro, right?
			 // that means an inner macro will be needed to generate the function body, but a
			 // no arg function to jump to it from a function pointer
			 //
			 // $f(arg1, arg2...);
		 	 // stuff mut args back in pipe, like the old macro...
			 // depending on retval or not, a inner macro may put that back too, like in
			 // sandbox_inner
		 }

		 // possibly called by PARENT (and child):
		 // FIXME: am besten gleich: je nach direkt-c oder nicht die in Ruhe lassen und nen anderen
		 // wrapper nehmen
         fn $f($($x)*) {
		 	 // eigentlich braucht der Scheiß eh ein globales lock, schon wegen der pipes!
			 // des natürlich traurig... ma gucken ob sich das auf mehrere invocations scalieren
			 // lässt
			 // da:
			 /*
			extern crate sync;
			use sync::mutex::{StaticMutex, MUTEX_INIT};

			static LIBRARY_LOCK: StaticMutex = MUTEX_INIT;

			fn access_global_resource() {
    			let _ = LIBRARY_LOCK.lock();
    			unsafe { call_thread_unsafe_c_api(); }
			}
			*/


			// if child is 0 but pipe is set, just run the function, it was called child-side
			 if SANDCRUST_GLOBAL.cmd_send != 0 && SANDCRUST_GLOBAL.child == 0 {
				println!("moving into childmode");
			 	 $body
			} else {
					// parent mode, potentially freshly initialized
					println!("parent mode");
					let sandcrust = Sandcrust::new_global();
					// function pointer to newly created method
					let f = wrap_$f;
					f();

					// copy vars (that are typed via the function signature) to child via pipe, somehow
					// first copying a function pointer to $f_wrapped or some shit (that could actually
					// quite be enough)
					//
					// ... wait for shit to come back:
            		//handle_changed_vals!($($x)*);
					// then in return pipe via other macro collect all args that may have changed, this
					// time we even know the return value!
			}
		}
	};
}


// no retval
#[macro_export]
macro_rules! sandbox_no_ret {
     ($f:ident($($x:tt)*)) => {{
         sandbox_internal!(no_ret, $f($($x)*));
     }};
}
