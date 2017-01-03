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
}
