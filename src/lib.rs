//! Sandcrust (**Sand**boxing **C** in **Rust**) is a library that automatically executes wrapped
//! functions in a sandboxed process.
//!
//! This is a highly experimental prototype, **do not use in production!**
#![warn(missing_docs, missing_debug_implementations, trivial_casts, trivial_numeric_casts, unstable_features, unused_extern_crates, unused_results, unused_import_braces, unused_qualifications, variant_size_differences)]

extern crate nix;

extern crate bincode;
extern crate serde;

extern crate sandheap;

#[cfg(feature = "shm")]
extern crate memmap;


#[allow(unused_imports)]
#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate lazy_static;

use std::os::unix::io::FromRawFd;
use std::os::unix::io::AsRawFd;

use sandheap as sandbox;

// wrap pid_t in own type to avoid re-import problems with nix
#[doc(hidden)]
pub type SandcrustPid = ::nix::libc::pid_t;

// pub use because of https://github.com/rust-lang/rust/issues/31355
#[doc(hidden)]
pub use ::nix::unistd::ForkResult as SandcrustForkResult;


// fake data type to implement wrappers on, see below
#[doc(hidden)]
#[derive(Debug)]
pub struct SandcrustWrapper;

#[doc(hidden)]
pub use serde_derive::*;

pub use serde::{Serialize, Deserialize};

/// Default SHM size for Sandcrust.
///
/// Initialize with sandcrust_init_with_shm_size() or sandcrust_set_shm_size() to set a different size.
#[cfg(feature = "shm")]
pub static SANDCRUST_DEFAULT_SHM_SIZE: usize = 2097152;

use std::io::{Read,Write};


// main data structure for sandcrust
#[doc(hidden)]
#[derive(Debug)]
#[cfg(feature = "shm")]
pub struct Sandcrust {
	file_in: std::fs::File,
	file_out: std::fs::File,
	child: SandcrustPid,
	shm: ::memmap::Mmap,
	shm_offset: usize,
}

// main data structure for sandcrust
#[doc(hidden)]
#[derive(Debug)]
#[cfg(not(feature = "shm"))]
pub struct Sandcrust {
	file_in: std::io::BufWriter<std::fs::File>,
	file_out: std::io::BufReader<std::fs::File>,
	child: SandcrustPid,
}


// lazily initialized global Sandcrust object (via Deref magic) for global sandbox

#[cfg(not(feature = "shm"))]
lazy_static! {
	#[doc(hidden)]
	#[derive(Debug)]
	pub static ref SANDCRUST: ::std::sync::Arc<::std::sync::Mutex<Sandcrust>> = {
		std::sync::Arc::new(std::sync::Mutex::new(Sandcrust::fork_new()))
	};
}

#[cfg(feature = "shm")]
lazy_static! {
	#[doc(hidden)]
	#[derive(Debug)]
	pub static ref SANDCRUST: ::std::sync::Arc<::std::sync::Mutex<Sandcrust>> = {
		std::sync::Arc::new(std::sync::Mutex::new(Sandcrust::fork_new()))
	};
	static ref SANDCRUST_SHM_SIZE: ::std::sync::Mutex<usize> = ::std::sync::Mutex::new(SANDCRUST_DEFAULT_SHM_SIZE);
}

// Necessary, because once the child is initialized, we need a lightweight, non-locking check to
// run the original function.
// Changing this is protected by SANDCRUST's mutex.
#[doc(hidden)]
pub static mut SANDCRUST_INITIALIZED_CHILD: bool = false;


impl Sandcrust {
	/// New Sandcrust object for one time use.
	pub fn new() -> Sandcrust {
		let (fd_out, fd_in) = nix::unistd::pipe().expect("sandcrust: failed to set up pipe");

		#[cfg(feature = "shm")]
		let size = SANDCRUST_SHM_SIZE.lock().expect("sandcrust: failed to lock SANDCRUST_SHM_SIZE");

		#[cfg(feature = "shm")]
		let sandcrust = Sandcrust {
			file_in: unsafe { ::std::fs::File::from_raw_fd(fd_in) },
			file_out: unsafe { ::std::fs::File::from_raw_fd(fd_out) },
			child: 0,
			shm: memmap::Mmap::anonymous(*size, ::memmap::Protection::ReadWrite).expect("sandcrust: failed to set up SHM"),
			shm_offset: 0,
		};
		#[cfg(not(feature = "shm"))]
		let sandcrust = Sandcrust {
			file_in: std::io::BufWriter::new(unsafe { ::std::fs::File::from_raw_fd(fd_in) }),
			file_out: std::io::BufReader::new( unsafe { ::std::fs::File::from_raw_fd(fd_out) }),
			child: 0,
		};
		sandcrust
	}

	/// New Sandcrust object for global use.
	///
	/// Creates a pipe of pairs, forks and returns Sandcrust objects with the appropriate pipe
	/// ends bound to file_in and file_out.
	pub fn fork_new() -> Sandcrust {
		let (child_cmd_receive, parent_cmd_send) = ::nix::unistd::pipe().expect("sandcrust: failed to set up pipe");
		let (parent_result_receive, child_result_send) = ::nix::unistd::pipe().expect("sandcrust: failed to set up pipe");

		#[cfg(feature = "shm")]
		let size = SANDCRUST_SHM_SIZE.lock().expect("sandcrust: failed to lock SANDCRUST_SHM_SIZE");

		#[cfg(feature = "shm")]
		let shm = memmap::Mmap::anonymous(*size, ::memmap::Protection::ReadWrite).expect("sandcrust: failed to set up SHM");
		// get pid to check for parent termination
		let ppid = ::nix::unistd::getpid();
		let sandcrust = match ::nix::unistd::fork() {
			Ok(::nix::unistd::ForkResult::Parent { child, .. }) => {
				::nix::unistd::close(child_cmd_receive).expect("sandcrust: failed to close unused child read FD");
				::nix::unistd::close(child_result_send).expect("sandcrust: failed to close unused child write FD");
				#[cfg(feature = "shm")]
				let sandcrust = Sandcrust {
					file_in:  unsafe { ::std::fs::File::from_raw_fd(parent_cmd_send) },
					file_out: unsafe { ::std::fs::File::from_raw_fd(parent_result_receive) },
					child: child,
					shm: shm,
					shm_offset: 0,
				};
				#[cfg(not(feature = "shm"))]
				let sandcrust = Sandcrust {
					file_in: std::io::BufWriter::new(unsafe { ::std::fs::File::from_raw_fd(parent_cmd_send) }),
					file_out: std::io::BufReader::new( unsafe { ::std::fs::File::from_raw_fd(parent_result_receive) }),
					child: child,
				};
				sandcrust
			}
			Ok(::nix::unistd::ForkResult::Child) => {
				// On Linux, instruct the kernel to kill the child when parent exits.
				// Compare recorded PID to current parent process ID to eliminate race condition.
				// Solution courtesy of
				// https://stackoverflow.com/questions/284325/how-to-make-child-process-die-after-parent-exits
				#[cfg(target_os="linux")]
				{
					unsafe {
						if 0 != ::nix::libc::prctl(::nix::libc::PR_SET_PDEATHSIG, ::nix::libc::SIGHUP) {
							panic!("Setting prctl() failed!");
						}
					}
					if ::nix::unistd::getppid() != ppid {
						::std::process::exit(0);
					}
				}

				// on Unices other that Linux, poll for parent exit every 10 seconds
				// During normal operation this threat gets cleaned up on exit.
				#[cfg(all(not(target_os="linux"),unix))]
				thread::spawn(move | | {
					loop {
						if ::nix::unistd::getppid() != ppid {
							::std::process::exit(0);
						}
						thread::sleep(Duration::from_secs(10));
					}
				});


				// we overload the meaning of file_in / file_out for parent and child here, which is
				// not nice but might enable reuse of some methods
				::nix::unistd::close(parent_cmd_send).expect("sandcrust: failed to close unused parent write FD");
				::nix::unistd::close(parent_result_receive).expect("sandcrust: failed to close unused parent read FD");

				#[cfg(feature = "shm")]
				let sandcrust = Sandcrust {
					file_in: unsafe { ::std::fs::File::from_raw_fd(child_result_send) },
					file_out: unsafe { ::std::fs::File::from_raw_fd(child_cmd_receive) },
					child: 0,
					shm: shm,
					shm_offset: 0,
				};
				#[cfg(not(feature = "shm"))]
				let sandcrust = Sandcrust {
					file_in: std::io::BufWriter::new(unsafe { ::std::fs::File::from_raw_fd(child_result_send) }),
					file_out: std::io::BufReader::new(unsafe { ::std::fs::File::from_raw_fd(child_cmd_receive) }),
					child: 0,
				};
				sandcrust
			}
			Err(e) => panic!("sandcrust: fork() failed with error {}", e),
		};
		sandcrust
	}

	/// Check if the process is unintialized child process and run child loop.
	///
	/// As noted above, modifications to static mut SANDCRUST_INITIALIZED_CHILD are protected by the mutex
	/// held on the global Sandcrust object.
	pub fn initialize_child(&mut self) {
		if !unsafe { SANDCRUST_INITIALIZED_CHILD } && self.child == -1 {
			// Sandbox was terminated, respawn if feature enabled, else fail
			#[cfg(feature = "auto_respawn")]
			self.respawn();
			#[cfg(not(feature = "auto_respawn"))]
			panic!("attempted to call sandboxed function after Sandbox termination");
		}
		if !unsafe { SANDCRUST_INITIALIZED_CHILD } && self.child == 0 {
			unsafe { SANDCRUST_INITIALIZED_CHILD = true };
			self.run_child_loop();
		}
	}


	/// Wrapper to set up an external sandbox.
	pub fn setup_sandbox(&self) {
		#[cfg(not(feature = "shm"))]
		{
			let file_in = self.file_in.get_ref().as_raw_fd();
			let file_out = self.file_out.get_ref().as_raw_fd();
			sandbox::setup(file_in, file_out);
		}
		#[cfg(feature = "shm")]
		{
			let file_in = self.file_in.as_raw_fd();
			let file_out = self.file_out.as_raw_fd();
			sandbox::setup(file_in, file_out);
		}
	}


	/// Client side loop.
	///
	/// Take unsigned number from comand pipe, convert to function pointer and run it.
	/// If command number is 0, exit the child process.
	fn run_child_loop(&mut self) {
		self.setup_sandbox();
		loop {
			let func: fn(&mut Sandcrust) = self.get_func_ptr();
			func(self);
		}
	}


	/// Waits for process with child pid.
	pub fn join_child(&mut self) {
		match nix::sys::wait::waitpid(self.child, None) {
			Ok(_) => { self.child = -1 }
			Err(e) => panic!("sandcrust waitpid() failed with error {}", e),
		}
	}


	/// Put variable in pipe.
	pub fn put_var_in_fifo<T: ::serde::Serialize>(&mut self, var: T) {
		::bincode::serialize_into(&mut self.file_in,
												&var,
												::bincode::Infinite)
												.expect("sandcrust: failed to put variable in fifo");
	}


	/// Restore variable from pipe.
	pub fn restore_var_from_fifo<T: ::serde::Deserialize>(&mut self) -> T {
		::bincode::deserialize_from(&mut self.file_out, ::bincode::Infinite)
											.expect("sandcrust: failed to read variable from fifo")
	}


	/// Reset SHM offset.
	#[cfg(feature = "shm")]
	pub fn reset_shm_offset(&mut self) {
		self.shm_offset = 0;
	}


	/// Wait for return signal from child.
	#[cfg(feature = "shm")]
	pub fn await_return(&mut self) {
		let mut buf = [0u8];
		let _ = self.file_out.read(&mut buf).expect("sandcrust: failed to read ready-signal");
	}


	/// Signal sucessful IPC return to parent.
	#[cfg(feature = "shm")]
	pub fn signal_return(&mut self) {
		let _ = self.file_in.write_all(b"1").expect("sandcrust: ready-signal write failed");
	}

	/// Transmit function pointer to child.
	pub fn put_func_ptr(&mut self, func: fn(&mut Sandcrust))  {
		unsafe {
			let func_ptr: *const u8 = ::std::mem::transmute(func);
			#[cfg(target_pointer_width = "32")]
			let buf: [u8; 4] = unsafe{ std::mem::transmute(func_ptr)};
			//let buf = std::slice::from_raw_parts(func_ptr, 4);
			#[cfg(target_pointer_width = "64")]
			let buf: [u8; 8] = std::mem::transmute(func_ptr);
			//let buf = std::slice::from_raw_parts(func_ptr, 8);
			let _ = self.file_in.write_all(&buf).expect("sandcrust: failed to send func ptr");
		}
	}

	/// Receive function pointer.
	pub fn get_func_ptr(&mut self) -> fn(&mut Sandcrust)  {
		#[cfg(target_pointer_width = "32")]
		let mut buf = [0u8; 4];
		#[cfg(target_pointer_width = "64")]
		let mut buf = [0u8; 8];
		self.file_out.read_exact(&mut buf).expect("sandcrust: failed to read func ptr");
		let func_ptr: *const u8 = unsafe { std::mem::transmute(buf) };
		let func: fn(&mut Sandcrust) = unsafe { std::mem::transmute(func_ptr) };
		func
	}

	/// Put variable.
	pub fn put_var<T: ::serde::Serialize>(&mut self, var: T) {
		#[cfg(not(feature = "shm"))]
		#[inline]
		self.put_var_in_fifo(var);
		#[cfg(feature = "shm")]
		{
			let remaining_mem: usize = self.shm.len() - self.shm_offset;

			match ::bincode::serialized_size_bounded(&var, remaining_mem as u64) {
				Some(size) => {
					let mut mem = unsafe { self.shm.as_mut_slice() };
					let mut window = &mut mem[self.shm_offset..];
					::bincode::serialize_into(&mut window,
												&var,
												::bincode::Bounded(remaining_mem as u64))
												.expect("sandcrust: failed to put variable in shm");
					self.shm_offset += size as usize;
				},
				None => panic!("sandcrust: SHM out of memory!"),
			}
		}
	}


	/// Get variable.
	pub fn restore_var<T: ::serde::Deserialize>(&mut self) -> T {
		#[cfg(not(feature = "shm"))]
		#[inline]
		{
			self.restore_var_from_fifo()
		}
		#[cfg(feature = "shm")]
		{
		let mem = unsafe { self.shm.as_slice() };
		let window = &mem[self.shm_offset..];
		::bincode::deserialize(window).expect("sandcrust: failed to read variable from shm")
		}
	}


	/// FIXME: ugly clude b/c otherwise the compiler won't know that var (being type X) is
	/// serializeable
	/// Either find some trait magic to shut it up, or combine in macro
	#[cfg(feature = "shm")]
	pub fn update_shm_offset<T: ::serde::Serialize>(&mut self, var: T) {
		self.shm_offset += ::bincode::serialized_size(&var) as usize;
	}


	/// Custom restore function for byte vectors.
	#[cfg(all(feature = "custom_vec", not(feature = "shm")))]
	pub fn put_byte_vector(&mut self, vector: &[u8]) {
		let size = vector.len();

		// put size first
		let size_u64 = size as u64;
		let size_arr: [u8; 8] = unsafe{ std::mem::transmute(size_u64)};
		let _ = self.file_in.write_all(&size_arr).expect("sandcrust: failed to send vector size");

		// put data
		let _ = self.file_in.write_all(&vector[..]).expect("sandcrust: failed to send vector data");
	}

	/// Custom restore function for byte vectors.
	#[cfg(all(feature = "custom_vec", feature = "shm"))]
	pub fn put_byte_vector(&mut self, vector: &[u8]) {
		let size = vector.len();

		// check remaining memory
		let remaining_mem: usize = self.shm.len() - self.shm_offset;
		if remaining_mem < (size + 8) {
			panic!("sandcrust: too little remaining memory to put vector");
		}

		let mut mem = unsafe { self.shm.as_mut_slice() };

		// put size first
		{
			let size_u64 = size as u64;
			let size_arr: [u8; 8] = unsafe{ std::mem::transmute(size_u64)};
			let end = self.shm_offset + 8;
			let mut window = &mut mem[self.shm_offset..end];
			window.copy_from_slice(&size_arr);
			self.shm_offset += 8;
		}

		// put data
		let end = self.shm_offset + size;
		let mut window = &mut mem[self.shm_offset..end];
		window.copy_from_slice(&vector);
		self.shm_offset += size;
	}

	/// Custom put function for byte vectors.
	#[cfg(all(feature = "custom_vec", feature = "shm"))]
	pub fn restore_byte_vector(&mut self) -> Vec<u8> {
		// restore size
		let mem = unsafe { self.shm.as_mut_slice() };
		let end = self.shm_offset + 8;
		let window = &mem[self.shm_offset..end];
		// necessary, because a slice is a pointer and a size
		let mut size_arr: [u8; 8] = [0u8; 8];
		size_arr.copy_from_slice(&window);
		let size_u64: u64 = unsafe { std::mem::transmute(size_arr) };
		let size = size_u64 as usize;

		self.shm_offset += 8;

		// fill new vector
		let mut new_vec = vec![0u8; size];
		{
			let vec_slice = &mut new_vec[..];
			let end = self.shm_offset + size;
			let window = &mem[self.shm_offset..end];
			vec_slice.copy_from_slice(window);
		}
		self.shm_offset += size;
		new_vec
	}

	/// Custom put function for byte vectors.
	#[cfg(all(feature = "custom_vec", not(feature = "shm")))]
	pub fn restore_byte_vector(&mut self) -> Vec<u8> {
		// restore size
		let mut buf = [0u8; 8];
		self.file_out.read_exact(&mut buf).expect("sandcrust: failed to read vector size");
		let size_u64: u64 = unsafe { std::mem::transmute(buf) };
		let size = size_u64 as usize;

		// fill new vector
		let mut new_vec = vec![0u8; size];
		self.file_out.read_exact(&mut new_vec[..]).expect("sandcrust: failed to read func ptr");
		new_vec
	}

	/// Send '0' command pointer to child loop, causing child to shut down, and collect the child's
    /// exit status.
	pub fn terminate_child(&mut self) {
		let func: fn(&mut Sandcrust) = child_terminate;
		self.put_func_ptr(func);
		#[cfg(not(feature = "shm"))]
		self.flush_pipe();
		self.join_child();
	}


	/// Set child attribute to acquired value.
	pub fn set_child(&mut self, child: SandcrustPid) {
		self.child = child;
	}


	/// Respawn sandcrust, setting up new Sandbox.
	fn respawn(&mut self) {
		let new_sandcrust = Sandcrust::fork_new();
		self.file_in = new_sandcrust.file_in;
		self.file_out = new_sandcrust.file_out;
		self.child = new_sandcrust.child;
		#[cfg(feature = "shm")]
		{
			self.shm = new_sandcrust.shm;
			self.shm_offset = 0;
		}
	}

	/// Wrap fork for use in one-time sandbox macro to avoid exporting nix.
	pub fn fork(&self) -> Result<SandcrustForkResult, ::nix::Error> {
		nix::unistd::fork()
	}

	/// Flush Writer pipe to clear buffer.
	#[cfg(not(feature = "shm"))]
	pub fn flush_pipe(&mut self) {
		self.file_in.flush().expect("sandcrust: write flush failed");
	}
}

/// Store potentially changed vars into the pipe from child to parent.
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "custom_vec"))]
macro_rules! sandcrust_store_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var(&$head); };
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var(&$head);
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => { };
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => { };
	($sandcrust:ident, mut $head:ident : $typo:ty ) => { };
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// Store potentially changed vars into the pipe from child to parent.
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "custom_vec")]
macro_rules! sandcrust_store_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut Vec<u8>) => { $sandcrust.put_byte_vector(&$head); };
	($sandcrust:ident, $head:ident : &mut Vec<u8>, $($tail:tt)+) => {
		$sandcrust.put_byte_vector(&$head);
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var(&$head); };
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var(&$head);
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => { };
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => { };
	($sandcrust:ident, mut $head:ident : $typo:ty ) => { };
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		sandcrust_store_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}

/// Store potentially changed vars into the pipe from child to parent.
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_store_changed_vars {
	($sandcrust:ident, &mut $head:ident) => { $sandcrust.put_var($head); };
	($sandcrust:ident, &mut $head:ident, $($tail:tt)*) => {
		$sandcrust.put_var($head);
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


/// Restore potentially changed vars from pipe in the parent after IPC call.
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "shm"))]
macro_rules! sandcrust_restore_changed_vars {
	// only restore mut types
	($sandcrust:ident, &mut $head:ident) => {
		$head = $sandcrust.restore_var();
	};
	($sandcrust:ident, &mut $head:ident, $($tail:tt)+) => {
		$head = $sandcrust.restore_var();
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


/// Restore potentially changed vars from pipe in the parent after IPC call.
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "shm")]
macro_rules! sandcrust_restore_changed_vars {
	// only restore mut types
	($sandcrust:ident, &mut $head:ident) => {
		$head = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, &mut $head:ident, $($tail:tt)+) => {
		$head = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
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


/// Restore potentially changed vars from pipe in the parent after IPC call.
///
/// Global version - this would be a merge candidate with sandcrust_restore_changed_vars,
/// but inside the function &mut vars need to be dereferenced.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "custom_vec", not(feature = "shm")))]
macro_rules! sandcrust_restore_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut Vec<u8>) => {
		*$head = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>, $($tail:tt)+) => {
		*$head = $sandcrust.restore_byte_vector();
		sandcrust_restore_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		*$head = $sandcrust.restore_var();
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		*$head = $sandcrust.restore_var();
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


/// Restore potentially changed vars from pipe in the parent after IPC call.
///
/// Global version - this would be a merge candidate with sandcrust_restore_changed_vars,
/// but inside the function &mut vars need to be dereferenced.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "custom_vec", feature = "shm"))]
macro_rules! sandcrust_restore_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut Vec<u8>) => {
		*$head = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>, $($tail:tt)+) => {
		*$head = $sandcrust.restore_byte_vector();
		sandcrust_restore_changed_vars_global!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		*$head = $sandcrust.restore_var();
		$sandcrust.update_shm_offset($head);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		*$head = $sandcrust.restore_var();
		$sandcrust.update_shm_offset($head);
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
/// Restore potentially changed vars from pipe in the parent after IPC call.
///
/// Global version - this would be a merge candidate with sandcrust_restore_changed_vars,
/// but inside the function &mut vars need to be dereferenced.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "shm"), not(feature = "custom_vec")))]
macro_rules! sandcrust_restore_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		*$head = $sandcrust.restore_var();
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		*$head = $sandcrust.restore_var();
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


/// Restore potentially changed vars from pipe in the parent after IPC call.
///
/// Global version - this would be a merge candidate with sandcrust_restore_changed_vars,
/// but inside the function &mut vars need to be dereferenced.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "shm", not(feature = "custom_vec")))]
macro_rules! sandcrust_restore_changed_vars_global {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		*$head = $sandcrust.restore_var();
		$sandcrust.update_shm_offset($head);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		*$head = $sandcrust.restore_var();
		$sandcrust.update_shm_offset($head);
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

/// Push function arguments to global client in case they have changed since forking.
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "custom_vec"))]
macro_rules! sandcrust_push_function_args {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var(&*$head); };
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var(&*$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => { $sandcrust.put_var($head); };
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		$sandcrust.put_var($head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => { $sandcrust.put_var($head); };
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// Push function arguments to global client in case they have changed since forking.
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "custom_vec")]
macro_rules! sandcrust_push_function_args {
	($sandcrust:ident, $head:ident : &mut [u8]) => { $sandcrust.put_byte_vector(&*$head); };
	($sandcrust:ident, $head:ident : &mut [u8], $($tail:tt)+) => {
		$sandcrust.put_byte_vector(&*$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &[u8]) => { $sandcrust.put_byte_vector($head); };
	($sandcrust:ident, $head:ident : &[u8], $($tail:tt)+) => {
		$sandcrust.put_byte_vector($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>) => { $sandcrust.put_byte_vector(&*$head); };
	($sandcrust:ident, $head:ident : &mut Vec<u8>, $($tail:tt)+) => {
		$sandcrust.put_byte_vector(&*$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &Vec<u8>) => { $sandcrust.put_byte_vector($head); };
	($sandcrust:ident, $head:ident : &Vec<u8>, $($tail:tt)+) => {
		$sandcrust.put_byte_vector($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : Vec<u8>, $($tail:tt)+) => {
		$sandcrust.put_byte_vector(&$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : Vec<u8> ) => {
		$sandcrust.put_byte_vector(&$head);
	};
	($sandcrust:ident, mut $head:ident : Vec<u8>, $($tail:tt)+) => {
		$sandcrust.put_byte_vector(&$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, mut $head:ident : Vec<u8> ) => {
		$sandcrust.put_byte_vector(&$head);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty) => { $sandcrust.put_var(&*$head); };
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var(&*$head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => { $sandcrust.put_var($head); };
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		$sandcrust.put_var($head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => { $sandcrust.put_var($head); };
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		$sandcrust.put_var($head);
		sandcrust_push_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// Pull function arguments in global client.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "custom_vec"), not(feature = "shm")))]
macro_rules! sandcrust_pull_function_args {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		let mut $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => {
		let $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		let $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => {
		let mut $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}



/// Pull function arguments in global client.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "custom_vec", not(feature = "shm")))]
macro_rules! sandcrust_pull_function_args {
	($sandcrust:ident, $head:ident : &mut [u8]) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &mut [u8],  $($tail:tt)+) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &[u8]) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &[u8], $($tail:tt)+) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>, $($tail:tt)+) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &Vec<u8>) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &Vec<u8>, $($tail:tt)+) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, mut $head:ident : Vec<u8>) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, mut $head:ident : Vec<u8>, $($tail:tt)+) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : Vec<u8>) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : Vec<u8>, $($tail:tt)+) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		let mut $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => {
		let $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		let $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => {
		let mut $head: $typo = $sandcrust.restore_var();
	};
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// Pull function arguments in global client.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "custom_vec", feature = "shm"))]
macro_rules! sandcrust_pull_function_args {
	($sandcrust:ident, $head:ident : &mut [u8]) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &mut [u8], $($tail:tt)+) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &[u8]) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &[u8], $($tail:tt)+) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &mut Vec<u8>, $($tail:tt)+) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &Vec<u8>) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : &Vec<u8>, $($tail:tt)+) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, mut $head:ident : Vec<u8>) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, mut $head:ident : Vec<u8>, $($tail:tt)+) => {
		let mut $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : Vec<u8>) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
	};
	($sandcrust:ident, $head:ident : Vec<u8>, $($tail:tt)+) => {
		let $head: Vec<u8> = $sandcrust.restore_byte_vector();
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset($head);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset($head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// Pull function arguments in global client.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "custom_vec"), feature = "shm"))]
macro_rules! sandcrust_pull_function_args {
	($sandcrust:ident, $head:ident : &mut $typo:ty) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, $head:ident : &mut $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : &$typo:ty) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, $head:ident : &$typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty, $($tail:tt)+) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, $head:ident : $typo:ty ) => {
		let $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty ) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
	};
	($sandcrust:ident, mut $head:ident : $typo:ty, $($tail:tt)+) => {
		let mut $head: $typo = $sandcrust.restore_var();
		$sandcrust.update_shm_offset(&$head);
		sandcrust_pull_function_args!($sandcrust, $($tail)+);
	};
	($sandcrust:ident, ) => {};
}


/// Run function, gathering return value if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "shm"), not(feature = "custom_vec")))]
macro_rules! sandcrust_run_func_global {
	(has_ret, $has_vec:ident, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = sandcrust_strip_types!($f($($x)*));
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.put_var(&retval);
		$sandcrust.flush_pipe();
	};
	(no_ret, no_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		sandcrust_strip_types!($f($($x)*));
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.flush_pipe();
	};
}

/// Run function, gathering return value if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "shm", not(feature = "custom_vec")))]
macro_rules! sandcrust_run_func_global {
	(has_ret, $has_vec:ident, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = sandcrust_strip_types!($f($($x)*));
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.put_var(&retval);
		$sandcrust.signal_return();
		$sandcrust.reset_shm_offset();

	};
	(no_ret, no_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		sandcrust_strip_types!($f($($x)*));
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.signal_return();
	};
}


/// Run function, gathering return value if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "shm"), feature = "custom_vec"))]
macro_rules! sandcrust_run_func_global {
	(has_ret, has_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = sandcrust_strip_types!($f($($x)*));
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.put_byte_vector(&retval);
		$sandcrust.flush_pipe();
	};
	(has_ret, no_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = sandcrust_strip_types!($f($($x)*));
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.put_var(&retval);
		$sandcrust.flush_pipe();
	};
	(no_ret, no_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		sandcrust_strip_types!($f($($x)*));
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.flush_pipe();
	};
}

/// Run function, gathering return value if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "shm", feature = "custom_vec"))]
macro_rules! sandcrust_run_func_global {
	(has_ret, has_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = sandcrust_strip_types!($f($($x)*));
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.put_byte_vector(&retval);
		$sandcrust.signal_return();
		$sandcrust.reset_shm_offset();

	};
	(has_ret, no_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = sandcrust_strip_types!($f($($x)*));
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.put_var(&retval);
		$sandcrust.signal_return();
		$sandcrust.reset_shm_offset();

	};
	(no_ret, no_vec, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		sandcrust_strip_types!($f($($x)*));
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars_global!($sandcrust, $($x)*);
		$sandcrust.signal_return();
	};
}


/// Run function, gathering return value if available.
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "shm"))]
macro_rules! sandcrust_run_func {
	(has_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = $f($($x)*);
		sandcrust_store_changed_vars!($sandcrust, $($x)*);
		$sandcrust.put_var(&retval);
		$sandcrust.flush_pipe();
	};
	(no_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		$f($($x)*);
		sandcrust_store_changed_vars!($sandcrust, $($x)*);
		$sandcrust.flush_pipe();
	};
}


/// Run function, gathering return value if available.
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "shm")]
macro_rules! sandcrust_run_func {
	(has_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		let retval = $f($($x)*);
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars!($sandcrust, $($x)*);
		$sandcrust.put_var(&retval);
		$sandcrust.signal_return();
		$sandcrust.reset_shm_offset();

	};
	(no_ret, $sandcrust:ident, $f:ident($($x:tt)*)) => {
		$f($($x)*);
		$sandcrust.reset_shm_offset();
		sandcrust_store_changed_vars!($sandcrust, $($x)*);
		$sandcrust.signal_return();
	};
}


/// Collect return value in parent, if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "custom_vec", not(feature = "shm")))]
macro_rules! sandcrust_collect_ret {
	(has_ret, has_vec, $rettype:ty , $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_byte_vector();
		retval
	}};
	(no_ret, no_vec, $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, no_vec,  $rettype:ty, $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var();
		retval
	}};
	(no_ret, no_vec,  $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, $sandcrust:ident) => {{
		let retval = $sandcrust.restore_var();
		$sandcrust.join_child();
		retval
	}};
	(no_ret, $sandcrust:ident) => {
		$sandcrust.join_child();
	};
}


/// Collect return value in parent, if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(feature = "custom_vec", feature = "shm"))]
macro_rules! sandcrust_collect_ret {
	(has_ret, has_vec, $rettype:ty , $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_byte_vector();
		$sandcrust.reset_shm_offset();
		retval
	}};
	(no_ret, no_vec, $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, no_vec,  $rettype:ty, $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var();
		$sandcrust.reset_shm_offset();
		retval
	}};
	(no_ret, no_vec,  $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, $sandcrust:ident) => {{
		let retval = $sandcrust.restore_var();
		$sandcrust.reset_shm_offset();
		$sandcrust.join_child();
		retval
	}};
	(no_ret, $sandcrust:ident) => {
		$sandcrust.join_child();
	};
}


/// Collect return value in parent, if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "custom_vec"), not(feature = "shm")))]
macro_rules! sandcrust_collect_ret {
	(has_ret, has_vec, $rettype:ty , $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var();
		retval
	}};
	(no_ret, no_vec, $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, no_vec,  $rettype:ty, $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var();
		retval
	}};
	(no_ret, no_vec,  $rettype:ty, $sandcrust:ident) => { () };
	(has_ret, $sandcrust:ident) => {{
		let retval = $sandcrust.restore_var();
		$sandcrust.join_child();
		retval
	}};
	(no_ret, $sandcrust:ident) => {
		$sandcrust.join_child();
	};
}


/// Collect return value in parent, if available.
#[doc(hidden)]
#[macro_export]
#[cfg(all(not(feature = "custom_vec"), feature = "shm"))]
macro_rules! sandcrust_collect_ret {
	(has_ret, has_vec, $rettype:ty , $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var();
		$sandcrust.reset_shm_offset();
		retval
	}};
	(has_ret, no_vec,  $rettype:ty, $sandcrust:ident) => {{
		let retval: $rettype = $sandcrust.restore_var();
		$sandcrust.reset_shm_offset();
		retval
	}};
	(no_ret, no_vec, $rettype:ty, $sandcrust:ident) => {{
		$sandcrust.reset_shm_offset();
	}};
	(has_ret, $sandcrust:ident) => {{
		let retval = $sandcrust.restore_var();
		$sandcrust.join_child();
		retval
	}};
	(no_ret, $sandcrust:ident) => {
		$sandcrust.join_child();
	};
}

/// Strip argument types from function definition for calling the function.
///
/// Matching hell, but there is nothing else to do because Push Down Accumulation is a necessity
/// (see https://danielkeep.github.io/tlborm/book/pat-push-down-accumulation.html#incremental-tt-munchers).
/// Unfortunately, using $body:expr seems to match a single macro defition, but fails to expand in a
/// subsequent macro.
#[doc(hidden)]
#[macro_export]
macro_rules! sandcrust_strip_types {
	(($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!(($($tail)+) -> ($f($($body)+, &mut $head))));
	(($head:ident : &mut $typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!(($($tail)+) -> ($f(&mut $head))));
	(($head:ident : &mut $typo:ty) -> ($f:ident($($body:tt)+))) => ($f($($body)+, &mut $head));
	(($head:ident : &mut $typo:ty) -> ($f:ident())) => ($f(&mut $head));

	(($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!(($($tail)+) -> ($f($($body)+, &$head))));
	(($head:ident : &$typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!(($($tail)+) -> ($f(&$head))));
	(($head:ident : &$typo:ty) -> ($f:ident($($body:tt)+))) => ($f($($body)+, &$head));
	(($head:ident : &$typo:ty) -> ($f:ident())) => ($f(&$head));

	((mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!(($($tail)+) -> ($f($($body)+, mut $head))));
	((mut $head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!(($($tail)+) -> ($f(mut $head))));
	((mut $head:ident : $typo:ty) -> ($f:ident($($body:tt)+))) => ($f($($body)+, $head));
	((mut $head:ident : $typo:ty) -> ($f:ident())) => ($f($head));

	(($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident($($body:tt)+))) => (sandcrust_strip_types!(($($tail)+) -> ($f($($body)+, $head))));
	(($head:ident : $typo:ty, $($tail:tt)+) -> ($f:ident())) => (sandcrust_strip_types!(($($tail)+) -> ($f($head))));
	(($head:ident : $typo:ty) -> ($f:ident($($body:tt)+))) => ($f($($body)+, $head));
	(($head:ident : $typo:ty) -> ($f:ident())) => ($f($head));

	($f:ident($($tail:tt)+)) => (sandcrust_strip_types!(($($tail)+) -> ($f())));
	($f:ident()) => ($f());
}


/// Internal abstraction for single run with and without return value.
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
				#[cfg(not(feature = "shm"))]
				sandcrust.flush_pipe();
				::std::process::exit(0);
			}
			Err(e) => panic!("sandcrust: fork() failed with error {}", e),
		};
		sandcrust_collect_ret!($has_retval, sandcrust)
	}};
}


/// Create global SandcrustWrapper Trait to update client arguments and run the function.
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "shm"))]
macro_rules! sandcrust_global_create_wrapper {
	($has_retval:ident, $has_vec:ident, fn $f:ident($($x:tt)*)) => {
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
			// This doesn't work unfortunately...
			// #[allow(non_snake_case)]
			fn $f(sandcrust: &mut $crate::Sandcrust);
		}

		// wrapper function generated to draw the right amount of args from pipe
		// before calling the whole function client-side
		// It would be awesome to bind this to the existing struct Sandcrust, however at the
		// expense of possible function name collisions.
		impl $f for $crate::SandcrustWrapper {
			fn $f(sandcrust: &mut $crate::Sandcrust) {
				// get updated mutable global variables, if any
				sandcrust_pull_global(sandcrust);

				sandcrust_pull_function_args!(sandcrust, $($x)*);
				sandcrust_run_func_global!($has_retval, $has_vec, sandcrust, $f($($x)*));
			}
		}
	};
}


/// Create global SandcrustWrapper Trait to update client arguments and run the function.
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "shm")]
macro_rules! sandcrust_global_create_wrapper {
	($has_retval:ident, $has_vec:ident, fn $f:ident($($x:tt)*)) => {
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
			// This doesn't work unfortunately...
			// #[allow(non_snake_case)]
			fn $f(sandcrust: &mut $crate::Sandcrust);
		}

		// wrapper function generated to draw the right amount of args from pipe
		// before calling the whole function client-side
		// It would be awesome to bind this to the existing struct Sandcrust, however at the
		// expense of possible function name collisions.
		impl $f for $crate::SandcrustWrapper {
			fn $f(sandcrust: &mut $crate::Sandcrust) {
				sandcrust.reset_shm_offset();
				// get updated mutable global variables, if any
				sandcrust_pull_global(sandcrust);

				sandcrust_pull_function_args!(sandcrust, $($x)*);
				sandcrust_run_func_global!($has_retval, $has_vec, sandcrust, $f($($x)*));
			}
		}
	};
}


/// Create global funtion definition in place of the original.
///
/// Possibly called by PARENT (and child):
/// FIXME: am besten gleich: je nach direkt-c oder nicht die in Ruhe lassen und nen anderen wrapper nehmen
#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "shm"))]
macro_rules! sandcrust_global_create_function {
	($has_retval:ident, $has_vec:ident, fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
			// as an initialized child, just run function
			if unsafe{SANDCRUST_INITIALIZED_CHILD} {
				$body
			} else {
					let mut sandcrust = SANDCRUST.lock().expect("sandcrust: failed to lock mutex on global object");
					// potenially completely unintialized, if we're the child on first access, run
					// child loop
					sandcrust.initialize_child();

					// function pointer to newly created method...
					let func: fn(&mut $crate::Sandcrust) = $crate::SandcrustWrapper::$f;
					sandcrust.put_func_ptr(func);

					// update any mutable global variables in the child
					sandcrust_push_global(&mut sandcrust);
					sandcrust_push_function_args!(sandcrust, $($x)*);
					sandcrust.flush_pipe();

					sandcrust_restore_changed_vars_global!(sandcrust, $($x)*);
					sandcrust_collect_ret!($has_retval, $has_vec, $rettype, sandcrust)
			}
	};
}


/// Create global funtion definition in place of the original.
///
/// Possibly called by PARENT (and child):
/// FIXME: am besten gleich: je nach direkt-c oder nicht die in Ruhe lassen und nen anderen wrapper nehmen
#[doc(hidden)]
#[macro_export]
#[cfg(feature = "shm")]
macro_rules! sandcrust_global_create_function {
	($has_retval:ident, $has_vec:ident, fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
			// as an initialized child, just run function
			if unsafe{SANDCRUST_INITIALIZED_CHILD} {
				$body
			} else {
					let mut sandcrust = SANDCRUST.lock().expect("sandcrust: failed to lock mutex on global object");
					// potenially completely unintialized, if we're the child on first access, run
					// child loop
					sandcrust.initialize_child();

					// update any mutable global variables in the child
					sandcrust.reset_shm_offset();
					sandcrust_push_global(&mut sandcrust);
					sandcrust_push_function_args!(sandcrust, $($x)*);

					// function pointer to newly created method...
					let func: fn(&mut $crate::Sandcrust) = $crate::SandcrustWrapper::$f;
					// in SHM version, send the function after putting args
					sandcrust.put_func_ptr(func);

					sandcrust.await_return();
					sandcrust.reset_shm_offset();
					sandcrust_restore_changed_vars_global!(sandcrust, $($x)*);
					sandcrust_collect_ret!($has_retval, $has_vec, $rettype, sandcrust)
			}
	};
}
/// Wrap a function.
///
/// # This macro can be used in two major ways:
///
/// * Wrap a function invocation with return value once.
/// * Wrap function definitions, thereby creating a persistent sandboxed child process that all invocations of the wrapped functions are executed in.
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
	(pub fn $f:ident($($x:tt)*) -> Vec<u8> $body:block ) => {
		sandcrust_global_create_wrapper!(has_ret, has_vec, fn $f($($x)*));
		pub fn $f($($x)*) -> Vec<u8> {
			sandcrust_global_create_function!(has_ret, has_vec, fn $f($($x)*) -> Vec<u8> $body)
		}
	};
	(pub fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
		sandcrust_global_create_wrapper!(has_ret, no_vec, fn $f($($x)*));
		pub fn $f($($x)*) -> $rettype {
			sandcrust_global_create_function!(has_ret, no_vec, fn $f($($x)*) -> $rettype $body)
		}
	};
	(pub fn $f:ident($($x:tt)*) $body:block ) => {
		sandcrust_global_create_wrapper!(no_ret, no_vec, fn $f($($x)*));
		pub fn $f($($x)*) {
			sandcrust_global_create_function!(no_ret, no_vec, fn $f($($x)*) -> i32 $body)
		}
	};
	(fn $f:ident($($x:tt)*) -> Vec<u8> $body:block ) => {
		sandcrust_global_create_wrapper!(has_ret, has_vec, fn $f($($x)*));
		fn $f($($x)*) -> Vec<u8> {
			sandcrust_global_create_function!(has_ret, has_vec, fn $f($($x)*) -> Vec<u8> $body)
		}
	};
	(fn $f:ident($($x:tt)*) -> $rettype:ty $body:block ) => {
		sandcrust_global_create_wrapper!(has_ret, no_vec, fn $f($($x)*));
		fn $f($($x)*) -> $rettype {
			sandcrust_global_create_function!(has_ret, no_vec, fn $f($($x)*) -> $rettype $body)
		}
	};
	(fn $f:ident($($x:tt)*) $body:block ) => {
		sandcrust_global_create_wrapper!(no_ret, no_vec, fn $f($($x)*));
		fn $f($($x)*) {
			sandcrust_global_create_function!(no_ret, no_vec, fn $f($($x)*) -> i32 $body)
		}
	};
}


/// Wrap a function without a return value once.
///
/// Unfortunately this is a necessary distinction because Rust cannot distinguish between functions
/// with and without return value from the function call.
///
/// # Examples
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
///
/// # Examples
/// ```no_run
/// use sandcrust::*;
///
/// sandcrust_init();
/// ```
pub fn sandcrust_init() {
	let mut sandcrust = SANDCRUST.lock().expect("sandcrust: init: failed to lock mutex on global object");
	if sandcrust.child == -1 {
		sandcrust.respawn();
	}
	if !unsafe { SANDCRUST_INITIALIZED_CHILD } && sandcrust.child == 0 {
		unsafe { SANDCRUST_INITIALIZED_CHILD = true };
		sandcrust.run_child_loop();
	}
}

/// Initialize sandcrust with a custom SHM size.
#[cfg(feature = "shm")]
pub fn sandcrust_init_with_shm_size(new_size: usize) {
	#[inline]
	sandcrust_set_shm_size(new_size);
	#[inline]
	sandcrust_init();
}

/// Set a custom SHM size.
#[cfg(feature = "shm")]
pub fn sandcrust_set_shm_size(new_size: usize) {
	let mut size = SANDCRUST_SHM_SIZE.lock().expect("sandcrust: failed to lock SANDCRUST_SHM_SIZE");
	*size = new_size;
}

/// Terminate the global child.
///
/// **Attention** calls to sandboxed functions after child termination will panic if the
/// "auto_respawn" compile time feature is not enabled.
///
/// # Examples
/// ```no_run
/// use sandcrust::*;
///
/// sandcrust_terminate();
/// ```
pub fn sandcrust_terminate() {
	let mut sandcrust = SANDCRUST.lock().expect("sandcrust: terminate: failed to lock mutex on global object");
	sandcrust.terminate_child();
}

/// child-side cleanup function that adheres to the wrapper function signature.
#[allow(unused_variables)]
fn child_terminate(sandcrust: &mut Sandcrust) {
	::std::process::exit(0);
}

/// Update mutable global variables.
///
/// The macro takes an extern block of static mut variables and generates functions that push/pull
/// updates of mutable global variables and shadow the stub function below.
/// These functions are always called independed from use of the macro (hence the stubs).
///
/// # Examples
/// ```no_run
/// #[macro_use]
/// extern crate sandcrust;
///
///
/// sandcrust_wrap_global!{
/// 	#[link(name = "linkname")]
/// 	extern {
/// 		static mut variable1: i32;
/// 		static mut variable2: u8;
/// 	}
/// }
/// # fn main() { }
/// ```
#[macro_export]
macro_rules! sandcrust_wrap_global {
	(#[$link_flag:meta] extern { $(static mut $name:ident: $typo:ty;)+ }) => {
		// re-gengerate the extern block
		#[$link_flag]
		extern {
			$(
			static mut $name: $typo;
			)+
		}

		fn sandcrust_push_global(sandcrust: &mut $crate::Sandcrust) {
			unsafe {
				$(
					sandcrust.put_var(&$name);
				)+
			}
		}

		fn sandcrust_pull_global(sandcrust: &mut $crate::Sandcrust) {
			$(
				unsafe{
					$name = sandcrust.restore_var();
					#[cfg(feature = "shm")]
					sandcrust.update_shm_offset(&$name);
				}
			)+
		}
	}
}


// Stub function that is overlayed in the sandcrust_wrap_global macro (if used)
#[doc(hidden)]
#[inline]
#[allow(unused_variables)]
pub fn sandcrust_pull_global(sandcrust: &mut Sandcrust) {
}


// Stub function that is overlayed in the sandcrust_wrap_global macro (if used)
#[doc(hidden)]
#[inline]
#[allow(unused_variables)]
pub fn sandcrust_push_global(sandcrust: &mut Sandcrust) {
}
