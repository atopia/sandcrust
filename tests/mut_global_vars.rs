#[macro_use]
extern crate sandcrust;
extern crate libc;

#[cfg(test)]
mod mut_global_vars {
	use sandcrust::*;
	sandcrust_wrap_global!{
		#[link(name = "readline")]
		extern {
			static mut rl_readline_version: ::libc::c_int;
		}
	}

	sandbox!{
		fn check_global(run: i32) {
			unsafe {
				if run == 1 {
					assert_eq!(rl_readline_version, 23);
				} else if run == 2 {
					assert_eq!(rl_readline_version, 42);
				}
			}
		}
	}

	#[test]
	fn run_check_global() {
		let mut run = 1;
		unsafe {
			rl_readline_version = 23;
		}
		check_global(run);
		run += 1;
		unsafe {
			rl_readline_version = 42;
		}
		check_global(run);
	}
}
