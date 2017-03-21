extern crate nix;

extern crate bincode;
extern crate serde;

extern crate sandheap;

#[allow(unused_imports)]
#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate lazy_static;

// hook into terminate signal
#[cfg(feature = "catch_signals")]
extern crate chan_signal;
#[cfg(feature = "catch_signals")]
extern crate chan;

use std::os::unix::io::FromRawFd;
use std::os::unix::io::AsRawFd;

use sandheap as sandbox;

// wrap pid_t in own type to avoid re-import problems with nix
#[doc(hidden)]
pub type SandcrustPid = ::nix::libc::pid_t;

// pub use because of https://github.com/rust-lang/rust/issues/31355
#[doc(hidden)]
pub use ::nix::unistd::ForkResult as SandcrustForkResult;


// fake datatype to implement wrappers on, see below
#[doc(hidden)]
pub struct SandcrustWrapper;

pub use serde_derive::*;
pub use serde::{Serialize, Deserialize};

// main data structure for sandcrust
#[doc(hidden)]
pub struct Sandcrust {
	file_in: ::std::fs::File,
	file_out: ::std::fs::File,
	child: SandcrustPid,
}


// lazily initialized global Sandcrust object (via Deref magic) for global sandbox
lazy_static! {
	#[doc(hidden)]
	pub static ref SANDCRUST: ::std::sync::Arc<::std::sync::Mutex<Sandcrust>> = {
		std::sync::Arc::new(std::sync::Mutex::new(Sandcrust::fork_new()))
	};
}


// necessary, because once the child is initialized, we need a lightweight, non-locking check to
// run the original function
// changing this is protected by SANDCRUST's mutex
#[doc(hidden)]
pub static mut SANDCRUST_INITIALIZED_CHILD: bool = false;


impl Sandcrust {
	/// new Sandcrust object for one time use
	pub fn new() -> Sandcrust {
		let (fd_out, fd_in) = nix::unistd::pipe().unwrap();
		Sandcrust {
			file_in: unsafe { ::std::fs::File::from_raw_fd(fd_in) },
			file_out: unsafe { ::std::fs::File::from_raw_fd(fd_out) },
			child: 0,
		}
	}

	/// new Sandcrust object for global use
	///
	/// creates a pipe of pairs, forks and returns Sandcrust objects with the appropriate pipe
	/// ends bound to file_in and file_out
	pub fn fork_new() -> Sandcrust {
		let (child_cmd_receive, parent_cmd_send) = ::nix::unistd::pipe().unwrap();
		let (parent_result_receive, child_result_send) = ::nix::unistd::pipe().unwrap();

		#[allow(unused_mut)]
		let mut sandcrust = match ::nix::unistd::fork() {
			Ok(::nix::unistd::ForkResult::Parent { child, .. }) => {
				// install signal handler for SIGINT and SIGTERM (feature enabled by default
				#[cfg(feature = "catch_signals")]
				{
				let signal = ::chan_signal::notify(&[::chan_signal::Signal::INT, ::chan_signal::Signal::TERM]);
				::std::thread::spawn(move | | handle_signal(signal));
				}
				Sandcrust {
					file_in: unsafe { ::std::fs::File::from_raw_fd(parent_cmd_send) },
					file_out: unsafe { ::std::fs::File::from_raw_fd(parent_result_receive) },
					child: child,
				}
			}
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

	/// check if the process is unintialized child process and run child loop
	///
	/// as noted above, modifications to static mut SANDCRUST_INITIALIZED_CHILD are protected by the mutex
	/// held on the global Sandcrust object
	pub fn initialize_child(&mut self) {
		if !unsafe { SANDCRUST_INITIALIZED_CHILD } && self.child == 0 {
			unsafe { SANDCRUST_INITIALIZED_CHILD = true };
			self.run_child_loop();
		}
	}


	/// wrapper to set up an external sandbox
	pub fn setup_sandbox(&self) {
		let file_in = self.file_in.as_raw_fd();
		let file_out = self.file_out.as_raw_fd();
		sandbox::setup(file_in, file_out);
	}


	/// client side loop
	///
	/// take unsigned number from comand pipe, convert to function pointer and run it
	/// if command number is 0, exit the child process
	fn run_child_loop(&mut self) {
		self.setup_sandbox();
		loop {
			#[cfg(target_pointer_width = "32")]
			let func_int: u32 = self.restore_var_from_fifo();
			#[cfg(target_pointer_width = "64")]
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


	/// waits for process with child pid
	pub fn join_child(&mut self) {
		match nix::sys::wait::waitpid(self.child, None) {
			Ok(_) => {}
			Err(e) => println!("sandcrust waitpid() failed with error {}", e),
		}
		self.child = 0;
	}


	/// put variable in pipe
	pub fn put_var_in_fifo<T: ::serde::Serialize>(&mut self, var: T) {
		::bincode::serialize_into(&mut self.file_in,
												&var,
												::bincode::Infinite)
			.unwrap();
	}


	/// restore variable from pipe
	pub fn restore_var_from_fifo<T: ::serde::Deserialize>(&mut self) -> T {
		::bincode::deserialize_from(&mut self.file_out, ::bincode::Infinite)
			.unwrap()
	}


	/// send '0' command pointer to child loop, causing child to shut down
	pub fn terminate_child(&mut self) {
		self.put_var_in_fifo(0u64);
		self.join_child();
	}


	/// set child attribute to acquired value
	pub fn set_child(&mut self, child: SandcrustPid) {
		self.child = child;
	}

	/// wrap fork for use in one-time sandbox macro to avoid exporting nix
	pub fn fork(&self) -> std::result::Result<SandcrustForkResult, ::nix::Error> {
		nix::unistd::fork()
	}

}


/// store potentially changed vars into the pipe from child to parent
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_store_changed_vars {
	($sandcrust:ident, &mut $head:ident) => { $sandcrust.put_var_in_fifo($head); };
	($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
		$sandcrust.put_var_in_fifo($head);
		sandcrust_store_changed_vars!($sandcrust, $($tail)*);
	};
	($sandcrust:ident, &$head:ident) => { };
	($sandcrust:ident, &$head:ident, $($tail:tt)+) => {
		sandcrust_store_changed_vars!($sandcrust, $($tail)*);
	};
	// actually, the stmt match (for directly passing values) is greedy and will match the next ident, too
	($sandcrust:ident, $head:stmt) => { };
	($sandcrust:ident, $head:stmt, $($tail:tt)+) => {
		sandcrust_store_changed_vars!($sandcrust, $($tail)*);
	};
	($sandcrust:ident, $head:ident) => { };
	($sandcrust:ident, $head:ident, $($tail:tt)+) => {
		sandcrust_store_changed_vars!($sandcrust, $($tail)*);
	};
	($sandcrust:ident, ) => {};
}


/// restore potentially changed vars from pipe in the parent after IPC call
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_restore_changed_vars {
	// only restore mut types
	($sandcrust:ident, &mut $head:ident) => {
		$head = $sandcrust.restore_var_from_fifo();
	};
	($sandcrust:ident, &mut $head:ident, $($tail:tt)+) => {
		$head = $sandcrust.restore_var_from_fifo();
		sandcrust_restore_changed_vars!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, &$head:ident) => { };
	($sandcrust:ident, &$head:ident, $($tail:tt)+) => { sandcrust_restore_changed_vars!($sandcrust, $($tail)+); };
	// actually, the stmt match (for directly passing values) is greedy and will match the next ident, too
	($sandcrust:ident, $head:stmt) => { };
	($sandcrust:ident, $head:stmt, $($tail:tt)+) => { sandcrust_restore_changed_vars!($sandcrust, $($tail)+); };
	($sandcrust:ident, $head:ident) => { };
	($sandcrust:ident, $head:ident, $($tail:tt)+) => { sandcrust_restore_changed_vars!($sandcrust, $($tail)+); };
	($sandcrust:ident, ) => {};
}


/// restore potentially changed vars from pipe in the parent after IPC call
///
/// global version - this would be a merge candidate with sandcrust_restore_changed_vars,
/// but inside the function &mut vars need to be dereferenced
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_restore_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		*$head = $sandcrust.restore_var_from_fifo();
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		*$head = $sandcrust.restore_var_from_fifo();
		sandcrust_restore_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => { };
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		sandcrust_restore_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		sandcrust_restore_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => { };
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		sandcrust_restore_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => { };
	($sandcrust:ident, ) => {};
}


/// push function arguments to global client in case they have changed since forking
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_push_function_args {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var_in_fifo(&*$head); };
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var_in_fifo(&*$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => { $sandcrust.put_var_in_fifo($head); };
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var_in_fifo($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var_in_fifo($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		$sandcrust.put_var_in_fifo($head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => { $sandcrust.put_var_in_fifo($head); };
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var_in_fifo($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// pull function arguments in global client
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_pull_function_args {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		let mut $head: $typo = $sandcrust.restore_var_from_fifo();
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var_from_fifo();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => {
		let $head: $typo = $sandcrust.restore_var_from_fifo();
	};
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var_from_fifo();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var_from_fifo();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		let $head: $typo = $sandcrust.restore_var_from_fifo();
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => {
		let mut $head: $typo = $sandcrust.restore_var_from_fifo();
	};
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var_from_fifo();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// run function, gathering return value if available
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_run_func {
	(has_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = $f($($x)*);
		sandcrust_store_changed_vars!($sandcrust, $($x)*);
		$sandcrust.put_var_in_fifo(&retval);
	};
	(no_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		$f($($x)*);
		sandcrust_store_changed_vars!($sandcrust, $($x)*);
	};
}


/// collect return value in parent, if available
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_collect_ret {
	(has_ret, $rettype:ty, $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var_from_fifo();
		retval
	}};
	(no_ret, $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, $sandcrust:ident) => {{
		let retval = $sandcrust.restore_var_from_fifo();
		$sandcrust.join_child();
		retval
	}};
	(no_ret, $sandcrust:ident) => {
		$sandcrust.join_child();
	};
}


/// strip argument types from function definition for calling the function
///
/// matching hell, but there is nothing else to do because Push Down Accumulation is a necessity
/// (see https://danielkeep.github.io/tlborm/book/pat-push-down-accumulation.html#incremental-tt-munchers)
/// unfortunately, using $head:expr seems to match a single macro defition, but fails to expand in a
///  subsequent macro
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_strip_types {
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, &mut $head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f(&mut $head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, &mut $head)));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &mut $typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f(&mut $head)));

	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, &$head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f(&$head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, &$head)));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : &$typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f(&$head)));

	($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, mut $head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f(mut $head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, $head)));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, (mut $head:ident : $typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f($head)));

	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($($body)+, $head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f($head))));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty) -> ($f:ident($($body:tt)+))) => ($called_macro!($has_retval, $sandcrust, $f($($body)+, $head)));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, ($head:ident : $typo:ty) -> ($f:ident())) => ($called_macro!($has_retval, $sandcrust, $f($head)));

	($called_macro:ident, $has_retval:ident, $sandcrust:ident, $f:ident($($tail:tt)+)) => (sandcrust_strip_types!($called_macro, $has_retval, $sandcrust, ($($tail)+) -> ($f())));
	($called_macro:ident, $has_retval:ident, $sandcrust:ident, $f:ident()) => ($called_macro!($has_retval, $sandcrust, $f()));
}


/// internal abstraction for single run with and without return value
#[doc(hidden)]
#[macro_export]
macro_rules! sandbox_internal {
	($has_retval:ident, $f:ident($($x:tt)*)) => {{
		let mut sandcrust = $crate::Sandcrust::new();
		match sandcrust.fork() {
			Ok($crate::SandcrustForkResult::Parent { child, .. }) => {
				sandcrust_restore_changed_vars!(sandcrust, $($x)*);
				sandcrust.set_child(child);
			},
			Ok($crate::SandcrustForkResult::Child) => {
				sandcrust.setup_sandbox();
				sandcrust_run_func!($has_retval, sandcrust, $f($($x)*));
				::std::process::exit(0);
			}
			Err(e) => panic!("sandcrust: fork() failed with error {}", e),
		};
		sandcrust_collect_ret!($has_retval, sandcrust)
	}};
}


/// create global SandcrustWrapper Trait to update client arguments and run the function
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_global_create_wrapper {
	($has_retval:ident, fn $f:ident($($x:tt)*)) => {
		// Fake trait to implement a function to use as a wrapper function.
		// FIXME: ideally this should be done by defining a struct (like SandcrustWrapper) in the macro,
		// but only once (#ifndef bla struct OnlyOnce; #define bla #endif - Style) and just adding
		// a method named $f to it - however I haven't been able to figure out how to check for an
		// existing definition at compile time.
		// Using a trait instead, because traits can be added to a data type defined (once) elsewhere.
		// However, the downside is polluting the trait namespace, potentially colliding with
		// existing traits when wrapping functions such as Clone, Drop, etc.
		//	a simple $f_wrapped won't do in any way: https://github.com/rust-lang/rust/issues/12249
		#[allow(non_camel_case_types)]
		trait $f {
			#[allow(non_snake_case)]
			fn $f(sandcrust: &mut $crate::Sandcrust);
		}

		// wrapper function generated to draw the right amount of args from pipe
		// before calling the whole function client-side
		// It would be awesome to bind this to the existing struct Sandcrust, however at the
		// expense of possible function name collisions.
		impl $f for $crate::SandcrustWrapper {
			fn $f(sandcrust: &mut $crate::Sandcrust) {
				//println!("look I got magic going!: {}", ::nix::unistd::getpid());
				sandcrust_pull_function_args!(sandcrust, $($x)*);
				sandcrust_strip_types!(sandcrust_run_func, $has_retval, sandcrust, $f($($x)*));
			}
		}
	};
}


/// create global funtion definition in place of the original
///
/// possibly called by PARENT (and child):
/// FIXME: am besten gleich: je nach direkt-c oder nicht die in Ruhe lassen und nen anderen wrapper nehmen
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_global_create_function {
	($has_retval:ident, fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
			// as an initialized child, just run function
			if unsafe{SANDCRUST_INITIALIZED_CHILD} {
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
						#[cfg(target_pointer_width = "32")]
						let func_int: u32 = ::std::mem::transmute(func);
						#[cfg(target_pointer_width = "64")]
						let func_int: u64 = ::std::mem::transmute(func);
						sandcrust.put_var_in_fifo(&func_int);
					}
					sandcrust_push_function_args!(sandcrust, $($x)*);
					sandcrust_restore_changed_vars_global!(sandcrust, $($x)*);
					sandcrust_collect_ret!($has_retval, $rettype, sandcrust)
			}
	};
}


/// wrap a function
///
/// # This macro can be used in two major ways:
///
/// * wrap a function invocation with return value once
/// * wrap function definitions, thereby creating a persistent sandboxed child process that all invocations of the wrapped functions are executed in
///
/// # Wrap a function invocation with return value once
/// For this to work, it is generally necessary to specify the return type explicitly as the
/// instrumentation can not infer it from the invocation.
///
/// ```
/// #[macro_use]
/// extern crate sandcrust;
///
/// fn base_ret() -> i32 {
///		let ret = 23;
///		ret
///	}
///
/// fn main() {
///		let local_ret: i32 = sandbox!(base_ret());
/// }
/// ```
/// # Wrap function definitons
///
/// ```
/// #[macro_use]
/// extern crate sandcrust;
///	use sandcrust::*;
///
///	sandbox!{
///		fn no_ret() {
///			;
///		}
///	}
///
///	sandbox!{
///		fn base_ret() -> i32 {
///			let ret = 23;
///			ret
///		}
/// }
///
///	fn main() {
///		no_ret();
///		let local_ret = base_ret();
///		sandcrust_terminate();
///	}
/// ```
#[macro_export]
macro_rules! sandbox {
	// retval, potentially args
	($f:ident($($x:tt)*)) => {{
		sandbox_internal!(has_ret, $f($($x)*))
	}};
	// (global-)wrap a function definition, transforming it
	(pub fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
		sandcrust_global_create_wrapper!(has_ret, fn $f($($x)*));
		pub fn $f($($x)*) -> $rettype {
			sandcrust_global_create_function!(has_ret, fn $f($($x)*) -> $rettype $body)
		}
	};
	(pub fn $f:ident($($x:tt)*) $body:block ) => {
		sandcrust_global_create_wrapper!(no_ret, fn $f($($x)*));
		pub fn $f($($x)*) {
			sandcrust_global_create_function!(no_ret, fn $f($($x)*) -> i32 $body)
		}
	};
	(fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
		sandcrust_global_create_wrapper!(has_ret, fn $f($($x)*));
		fn $f($($x)*) -> $rettype {
			sandcrust_global_create_function!(has_ret, fn $f($($x)*) -> $rettype $body)
		}
	};
	(fn $f:ident($($x:tt)*) $body:block ) => {
		sandcrust_global_create_wrapper!(no_ret, fn $f($($x)*));
		fn $f($($x)*) {
			sandcrust_global_create_function!(no_ret, fn $f($($x)*) -> i32 $body)
		}
	};
}


/// wrap a function without a return value once
///
/// unfortunately this is a necessary distinction because Rust cannot distinguish between functions
/// with and without return value from the function call
///
/// ```
/// #[macro_use]
/// extern crate sandcrust;
///
/// use sandcrust::*;
///
/// fn no_ret() {
///		;
/// }
///
/// fn main() {
///		sandbox_no_ret!(no_ret());
/// }
/// ```
#[macro_export]
macro_rules! sandbox_no_ret {
	($f:ident($($x:tt)*)) => {{
		sandbox_internal!(no_ret, $f($($x)*));
	}};
}


/// Explicitly initialize the stateful sandbox.
///
/// This is unnecessary during normal use, but useful to set up the sandboxing mechanism at a
/// defined point in program execution, e.g. before loading senstive data into the address space.
pub fn sandcrust_init() {
	#[allow(unused_variables)]
	let sandcrust = SANDCRUST.lock().unwrap();
}


/// terminate the global child
///
/// **Attention** calls to sandboxed functions after child termination will hang indefinitely
///
/// ```no_run
/// use sandcrust::*;
///
/// sandcrust_terminate();
/// ```
pub fn sandcrust_terminate() {
	let mut sandcrust = SANDCRUST.lock().unwrap();
	sandcrust.terminate_child();
}


/// handle SIGINT and SIGTERM
#[cfg(feature = "catch_signals")]
fn handle_signal(signal: ::chan::Receiver<::chan_signal::Signal>) {
	signal.recv().unwrap();
	let mut sandcrust = SANDCRUST.lock().unwrap();
	if sandcrust.child != 0 {
		sandcrust.terminate_child();
	}
	std::process::exit(0);
}
