extern crate sandcrust;
use sandcrust::*;

#[cfg(test)]
mod val_restore {
    use super::*;

    fn base_inc(a: &mut u8) {
        *a += 1;
    }


    #[test]
    fn base_test() {
        let mut a: u8 = 23;
        sandbox_no_ret!(base_inc(&mut a));
        assert!(a == 24);
    }

    fn second_to_first(a: &mut i32, b: i32) {
        *a = b;
    }

    #[test]
    fn second_to_first_test() {
        let mut a = 23;
        let b = 42;
        sandbox_no_ret!(second_to_first(&mut a, b));
        assert!(a == 42);
    }

    fn first_to_second(a: i32, b: &mut i32) {
        *b = a;
    }

    #[test]
    fn first_to_second_test() {
        let a = 23;
        let mut b = 42;
        sandbox_no_ret!(first_to_second(a, &mut b));
        assert!(b == 23);
    }

    fn mult_args_ref_direct_1(a: &i32, b: &mut i32, c: i32) {
        *b = a + c;
    }

    #[test]
    fn mult_args_ref_direct_1_test() {
        let a = 1;
        let mut b = 2;
        let c = 3;
        sandbox_no_ret!(mult_args_ref_direct_1(&a, &mut b, c));
        assert!(b == 4);
    }

    fn mult_args_ref_direct_2(a: i32, b: &i32, c: &mut i32) {
        *c = a + b;
    }

    #[test]
    fn mult_args_ref_direct_2_test() {
        let a = 1;
        let b = 2;
        let mut c = 7;
        sandbox_no_ret!(mult_args_ref_direct_2(a, &b, &mut c));
        assert!(c == 3);
    }

    fn mult_args_ref_direct_3(a: &mut i32, b: &i32, c: i32) {
        *a = b + c;
    }

    #[test]
    fn mult_args_ref_direct_3_test() {
        let mut a = 1;
        let b = 2;
        let c = 3;
        sandbox_no_ret!(mult_args_ref_direct_3(&mut a, &b, c));
        assert!(a == 5);
    }

    fn mult_mut_args_1(a: i32, b: &mut i32, c: &mut i32) {
        let d = a + 3;
        *c = a;
        *b = d;
    }

    #[test]
    fn mult_mut_args_1_test() {
        let a = 1;
        let mut b = 2;
        let mut c = 3;
        sandbox_no_ret!(mult_mut_args_1(a, &mut b, &mut c));
        assert!(b == 4);
        assert!(c == 1);
    }

    fn mult_mut_args_2(a: &mut i32, b: i32, c: &mut i32) {
        let d = b + 3;
        *a = d;
        *c = b;
    }

    #[test]
    fn mult_mut_args_2_test() {
        let mut a = 1;
        let b = 2;
        let mut c = 3;
        sandbox_no_ret!(mult_mut_args_2(&mut a, b, &mut c));
        assert!(a == 5);
        assert!(c == 2);
    }
}
