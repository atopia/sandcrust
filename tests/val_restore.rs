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
        sandbox_me!(base_inc(&mut a));
        assert!(a == 24);
    }

    fn second_to_first(a : &mut i32, b : i32) {
        *a = b;
    }

    #[test]
    fn second_to_first_test() {
        let mut a = 23;
        let b = 42;
        sandbox_me!(second_to_first(&mut a, b));
        assert!(a == 42);
    }
}
