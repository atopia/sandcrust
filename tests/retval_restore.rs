#[macro_use]
extern crate sandcrust;

#[cfg(test)]
mod retval_restore {

	fn no_ret() {}


	#[test]
	fn no_ret_test() {
		sandbox_no_ret!(no_ret());
	}

	fn base_ret() -> i32 {
		let ret = 23;
		ret
	}

	#[test]
	fn base_ret_test() {
		let local_ret: i32 = sandbox!(base_ret());
		assert_eq!(local_ret, 23);
	}

	fn second_base_ret(bla: &mut i32) -> i32 {
		let ret = 23;
		*bla = 7;
		ret
	}

	#[test]
	fn second_base_ret_test() {
		let mut bla = 22;
		let local_ret: i32 = sandbox!(second_base_ret(&mut bla));
		assert_eq!(local_ret, 23);
		assert_eq!(bla, 7);
	}
}
