extern crate sandcrust;
use sandcrust::*;

#[cfg(test)]
mod retval_restore {
    use super::*;

    fn no_ret() {
        ;
    }


    #[test]
    fn no_ret_test() {
        sandbox_me!(no_ret());
    }

    fn base_ret() -> i32 {
        let ret = 23;
        ret
    }

    #[test]
    fn base_ret_test() {
        let local_ret = sandbox_me!(base_ret());
        assert!(local_ret == 23);
    }

    fn second_base_ret(bla: &mut i32) -> i32 {
        let ret = 23;
        *bla = 7;
        ret
    }

    #[test]
    fn second_base_ret_test() {
        let mut bla = 22;
        let local_ret = sandbox_me!(second_base_ret(&mut bla));
        assert!(local_ret == 23);
        assert!(bla == 7);
    }
}
