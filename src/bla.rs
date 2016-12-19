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

macro_rules! process_var {
    () => {};
    ($head:ident) => {
        println!("single {}", $head);
    };
    ($head:ident, $($tail:ident),*) => {
        println!("shiny {}", $head);
        process_var!($($tail),*);
    };
        /*
    () => {};
    ($head:ident,$($tail:expr),* ) => {
        println!("ident head {}", head);
                //unsafe {
                //process_var!($x);
  //              let tempvar = $x;
                //let tempvar = $x;
//                let v = sandcrust.get_var_in_shm(tempvar);
 //               *v = *tempvar;
    //            }; // FIXME ends
        process_var!($($tail),*);
    };
    ($head:expr,$($tail:expr),* ) => {
        println!("expr head");
        process_var!($($tail),*);
    };
    */
}


#[macro_export]
macro_rules! sandbox_me {
     ($f:ident()) => {{
         println!("match empty");
         $f();
    }};
     ($f:ident($(&$x:ident ),*)) => {{
         println!("match ref");
         $f($($x),*);
     }};
     ($f:ident($(&mut $x:ident ),*)) => {{
    //($f:ident($($x:ident ),*)) => {{
        /*
        let mut size: usize = 8;
        $(
            size += size_of_val(&$x);
        )*
*/
 //       let mut sandcrust = Sandcrust::new(size);

            //$f($($x),*);
           process_var!($($x),*);
            exit(0);
    }}
}

fn print_a_b(a : &mut i32, b : &mut i32) {
    *b = *a;
}

fn eat_a_b(a: i32, mut b: i32) {
    b = a;
}

fn empty() {
    println!("this function args is none");
}

fn ref_to_a(a: i32) {
    println!("this function is passed a ref to {}", a);
}


pub fn main() {
    let mut a = 23;
    let mut b = 42;
    sandbox_me!(empty());
    sandbox_me!(ref_to_a(&a));
    sandbox_me!(print_a_b(&mut a, &mut b));
    //sandbox_me!(eat_a_b(a, b));
}
