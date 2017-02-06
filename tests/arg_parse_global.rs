#[macro_use]
extern crate sandcrust;


#[cfg(test)]
mod arg_parse_global {
	use sandcrust::*;

	sandbox!{fn empty() {}}

	#[test]
	fn empty_test() {
		empty();
	}


	sandbox!{
			fn by_value_simple(a: i32) {
			assert_eq!(a, 1);
		}
	}

	#[test]
	fn by_value_simple_test() {
		let a = 1;
		by_value_simple(a);
	}

	sandbox!{
		fn by_value_recursive(a: i32, b: i32) {
			assert_eq!(a, 2);
			assert_eq!(b, 3);
		}
	}

	#[test]
	fn by_value_recursive_test() {
		let a = 2;
		let b = 3;
		by_value_recursive(a, b);
	}

	sandbox!{
		fn by_mut_value_simple(mut a: i32) {
			a += 1;
			assert_eq!(a, 5);
		}
	}

	#[test]
	fn by_mut_value_simple_test() {
		let a = 4;
		by_mut_value_simple(a);
	}

	sandbox!{
		fn by_mixed_value_recursive(a: i32, mut b: i32) {
			if b > a {
				b = a;
			}
			assert_eq!(b, 5);
		}
	}

	#[test]
	fn by_mixed_value_recursive_test() {
		let a = 5;
		let b = 6;
		by_mixed_value_recursive(a, b);
	}

	sandbox!{
		fn by_reference_simple(a: &i32) {
			assert_eq!(*a, 7);
		}
	}

	#[test]
	fn by_reference_simple_test() {
		let a = 7;
		by_reference_simple(&a);
	}

	sandbox!{
		fn by_reference_recursive(a: &i32, b: &i32) {
			assert_eq!(*a, 8);
			assert_eq!(*b, 9);
		}
	}

	#[test]
	fn by_reference_recursive_test() {
		let a = 8;
		let b = 9;
		by_reference_recursive(&a, &b);
	}

	sandbox!{
		fn by_mut_reference_simple(a: &mut i32) {
			*a += 1;
		}
	}

	#[test]
	fn by_mut_reference_simple_test() {
		let mut a = 10;
		by_mut_reference_simple(&mut a);
	}

	sandbox!{
		fn by_mut_reference_recursive(a: &mut i32, b: &mut i32) {
			let swap = *a;
			*a = *b;
			*b = swap;
			assert_eq!(*a, 12);
			assert_eq!(*b, 11);
		}
	}

	#[test]
	fn by_mut_reference_recursive_test() {
		let mut a = 11;
		let mut b = 12;
		by_mut_reference_recursive(&mut a, &mut b);
	}

	#[test]
	fn kill_it() {
		sandcrust_terminate();
	}
}
