# citymapper-rs [![Build Status](https://travis-ci.org/alexjg/citymapper-rs.svg?branch=master)](https://travis-ci.org/alexjg/citymapper-rs) [![Crates.io](https://img.shields.io/crates/v/citymapper-rs.svg)](https://crates.io/crates/citymapper) [![Docs](https://docs.rs/citymapper/badge.svg)](https://docs.rs/citymapper-rs/0.1.0/citymapper-rs/)

This is a tiny library wrapping the citymapper API in a futures aware interface.

## Usage

Install the thing

    cargo install citymapper

Use the thing

```rust
extern crate chrono;
extern crate tokio_core;
extern crate citymapper;
use tokio_core::reactor::Core;

fn main() {
    let api_key = "<your api key>".to_string();
    let start_coord = (51.525246, 0.084672);
    let end_coord = (51.559098, 0.074503);
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = citymapper::ClientBuilder::new(&handle, api_key).build();
    let time_info = citymapper::TimeConstraint::arrival_by(
        chrono::Utc::now() + chrono::Duration::seconds(1800),
    );
    let response_future = client.travel_time(start_coord, end_coord, time_info);
    let response = core.run(response_future).unwrap();
    println!("Response: {:?}", response);
}
```

See the documentation for more details.

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

