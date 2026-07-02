#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Parse should never panic on any input
    let _ = nidhi::sf2::parse(data);
});
