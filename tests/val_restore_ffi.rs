extern crate sandcrust;
extern crate libc;

#[cfg(test)]
mod val_restore_ffi {
    use libc::{c_char, c_int};
    use std::ffi::CString;

    use sandcrust::*;

    extern {
        fn puts(s: *const c_char) -> c_int;
    }

    #[test]
    fn puts_test() {
        let greeting = CString::new("Hello libc").unwrap();
        unsafe {
            let gp = greeting.as_ptr();
            sandbox_me!(puts(gp));
        }
    }
}
