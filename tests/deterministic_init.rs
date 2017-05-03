extern crate sandcrust;

#[cfg(test)]
mod deterministic_init {
	use sandcrust::*;
	// initialized the sandbox at a deterministic point
	#[test]
	fn run_deterministic_init() {
		sandcrust_init();
	}
}
