extern crate sandcrust;
use sandcrust::*;

#[cfg(test)]
mod arg_parse {
    use super::*;

    fn empty() { }

    #[test]
    fn empty_test() {
        sandbox_me!(empty());
    }


    fn by_value_simple(a: i32) {
        assert!(a == 1);
    }

    #[test]
    fn by_value_simple_test() {
        let a = 1;
        sandbox_me!(by_value_simple(a));
    }

    fn by_value_recursive(a: i32, b: i32) {
        assert!(a == 2);
        assert!(b == 3);
    }

    #[test]
    fn by_value_recursive_test() {
        let a = 2;
        let b = 3;
        sandbox_me!(by_value_recursive(a, b));
    }

    fn by_mut_value_simple(mut a: i32) {
        a += 1;
        assert!(a == 5);
    }

    #[test]
    fn by_mut_value_simple_test () {
        let a = 4;
        sandbox_me!(by_mut_value_simple(a));
    }

    fn by_mixed_value_recursive(a: i32, mut b: i32) {
        if b > a {
            b = a;
        }
        assert!(b == 5);
    }

    #[test]
    fn by_mixed_value_recursive_test() {
        let a = 5;
        let b = 6;
        sandbox_me!(by_mixed_value_recursive(a, b));
    }

    fn by_reference_simple(a: &i32) {
        assert!(*a == 7);
    }

    #[test]
    fn by_reference_simple_test() {
        let a = 7;
        sandbox_me!(by_reference_simple(&a));
    }

    fn by_reference_recursive(a: &i32, b: &i32) {
        assert!(*a == 8);
        assert!(*b == 9);
    }

    #[test]
    fn by_reference_recursive_test() {
        let a = 8;
        let b = 9;
        sandbox_me!(by_reference_recursive(&a, &b));
    }

    fn by_mut_reference_simple(a: &mut i32) {
        *a += 1;
    }

    #[test]
    fn by_mut_reference_simple_test() {
        let mut a = 10;
        sandbox_me!(by_mut_reference_simple(&mut a));
    }

    fn by_mut_reference_recursive(a: &mut i32, b: &mut i32) {
        let swap = *a;
        *a = *b;
        *b = swap;
        assert!(*a == 12);
        assert!(*b == 11);
    }

    #[test]
    fn by_mut_reference_recursive_test() {
        let mut a = 11;
        let mut b = 12;
        sandbox_me!(by_mut_reference_recursive(&mut a, &mut b))
    }
}
