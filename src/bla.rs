pub use std::process::exit;
pub use std::mem::size_of_val;

use std::fs::{OpenOptions, remove_file};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}



// needed as a wrapper for all the imported uses
#[doc(hidden)]
pub struct Sandcrust {
    memptr: *mut u8,
}

impl Sandcrust {
    pub fn new(size: usize) -> Sandcrust {
        let size = size as u64;
        Sandcrust { memptr: 0 as *mut u8 }
    }

    pub unsafe fn get_var_in_shm<T>(&mut self, val: &T) -> *mut T {
        let size = size_of_val(val);
        let memptr_orig = self.memptr;
        self.memptr.offset(size as isize);
        memptr_orig as *mut T
    }
}


                //unsafe {
  //              let tempvar = $x;
                //let tempvar = $x;
//                let v = sandcrust.get_var_in_shm(tempvar);
 //               *v = *tempvar;
    //            }; // FIXME ends

#[macro_export]
macro_rules! sandbox_me {
    (&mut $head:ident) => {
        println!("single mut ref: {}", $head);
    };

    (&mut $head:ident, $($tail:tt)*) => {
        println!("process mut ref: {}", $head);
        sandbox_me!($($tail)*);
    };

    (&$head:ident) => {
        println!("single ref: {}", $head);
    };

    (&$head:ident, $($tail:tt)+) => {
        println!("process ref: {}", $head);
        sandbox_me!($($tail)*);
    };


    ($head:ident) => {
        println!("single var: {}", $head);
    };

    ($head:ident, $($tail:tt)+) => {
        println!("process var: {}", $head);
        sandbox_me!($($tail)*);
    };

     ($f:ident()) => {
         println!("match empty");
         $f();
    };
     ($f:ident($($t:tt)+)) => {
        sandbox_me!($($t)+);
        $f($($t)+);
     }
     /*
    ($f:ident($($x:ident ),*)) => {{
        let mut size: usize = 8;
        $(
            size += size_of_val(&$x);
        )*
        let mut sandcrust = Sandcrust::new(size);

            //$f($($x),*);
           process_var!($($x),*);
            exit(0);
    }}
*/
}

fn write_b_to_a(a : &mut i32, b : &mut i32) {
    *b = *a;
}

fn eat_a_b(a: i32, mut b: i32) {
    b = a;
}

fn empty() {
    println!("this function args is none");
}

fn ref_to_a(a: &i32) {
    println!("this function is passed a ref to {}", a);
}

fn take_a(a: i32) {
    println!("this function is passed {}", a);
}


pub fn main() {
    let mut a = 23;
    let mut b = 42;
    sandbox_me!(empty());
    sandbox_me!(take_a(a));
    sandbox_me!(ref_to_a(&a));
    sandbox_me!(ref_to_a(&mut b));
    sandbox_me!(write_b_to_a(&mut a, &mut b));
    sandbox_me!(eat_a_b(a, b));
}
