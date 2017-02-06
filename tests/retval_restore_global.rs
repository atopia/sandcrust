#[macro_use]
extern crate sandcrust;

#[cfg(test)]
mod retval_restore_global {
	use sandcrust::*;

	sandbox!{
        fn no_ret() {
            ;
        }
    }


	#[test]
	fn no_ret_test() {
		no_ret();
	}

	sandbox!{
        fn base_ret() -> i32 {
            let ret = 23;
            ret
        }
    }

	#[test]
	fn base_ret_test() {
		let local_ret = base_ret();
		assert_eq!(local_ret, 23);
	}

	sandbox!{
        fn second_base_ret(bla: &mut i32) -> i32 {
            let ret = 23;
            *bla = 7;
            ret
        }
    }

	#[test]
	fn second_base_ret_test() {
		let mut bla = 22;
		let local_ret = second_base_ret(&mut bla);
		assert_eq!(local_ret, 23);
		assert_eq!(bla, 7);
	}

	#[test]
	fn kill_it() {
		sandcrust_terminate();
	}
}
