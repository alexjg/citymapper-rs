extern crate citymapper;
extern crate chrono;
extern crate tokio_core;
#[macro_use]
extern crate slog;
extern crate slog_term;

use slog::Drain;
use tokio_core::reactor::Core;

fn main() {
    let api_key = "db1e0abcbf75352e33eae15a9a54dd8d".to_string();
    let start_coord = (51.525246,0.084672);
    let end_coord = (51.559098,0.074503);
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let mut builder = citymapper::ClientBuilder::new(&handle, api_key);
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let logger = slog::Logger::root(
        slog_term::FullFormat::new(plain).build().fuse(), o!()
        );
    let client = builder
        .with_logger(logger)
        .build();
    //let time_info = citymapper::TimeConstraint::arrivate(
    //chrono::Utc::now() + chrono::Duration::seconds(1800),
    //);
    let response_future = client.travel_time(start_coord, end_coord, None);
    let response = core.run(response_future).unwrap();
    println!("Response: {0}", response);
}
