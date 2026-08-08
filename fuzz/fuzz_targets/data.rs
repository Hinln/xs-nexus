#![no_main]

use libfuzzer_sys::fuzz_target;
use xs_protocol::DataHeader;

fuzz_target!(|input: &[u8]| {
    let _ = DataHeader::parse(input);
});
