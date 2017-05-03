extern crate sandcrust;

#[cfg(test)]
mod deterministic_sandbox_handling {
    use sandcrust::*;
	// initialized the sandbox at a deterministic point
	#[test]
	fn run_deterministic_init() {
        sandcrust_init();
	}

	#[test]
	#[cfg(feature = "auto_respawn")]
	fn run_deterministic_sandbox_handling() {
        sandcrust_init();
        sandcrust_terminate();
	}
}
