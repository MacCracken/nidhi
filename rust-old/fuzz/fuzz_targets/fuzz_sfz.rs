#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = core::str::from_utf8(data) {
        // Parse should never panic on any input
        if let Ok(sfz) = nidhi::sfz::parse(input) {
            // Conversion should never panic on parsed data
            let _ = sfz.to_zones(44100.0);
            let _ = sfz.to_instrument("fuzz", 44100.0);
        }
    }
});
