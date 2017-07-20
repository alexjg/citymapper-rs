extern crate citymapper;
extern crate tokio_core;
extern crate mockito;
extern crate chrono;
extern crate url;
extern crate spectral;
extern crate futures;
#[macro_use]
extern crate slog;
extern crate slog_term;

use tokio_core::reactor::{Core, Handle};
use spectral::prelude::*;
use mockito::mock;
use slog::Drain;

fn create_client(handle: &Handle) -> Box<citymapper::Client> {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let logger = slog::Logger::root(slog_term::FullFormat::new(plain).build().fuse(), o!());
    let api_key = "123456".to_string();
    let mut builder = citymapper::ClientBuilder::new(handle, api_key);
    let client = builder
        .with_base_url(url::Url::parse(mockito::SERVER_URL).unwrap())
        .with_logger(logger)
        .build();
    Box::new(client)
}


#[test]
fn test_returns_travel_time_correctly() {
    let _m = mock(
        "GET",
        mockito::Matcher::Regex(r"^/traveltime/\?.*$".to_string()),
    ).with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body("{\"travel_time_minutes\": 123}")
        .create();
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = create_client(&handle);
    let start_coord = (51.525246, 0.084672);
    let end_coord = (51.559098, 0.074503);
    let time_info = citymapper::TimeConstraint::arrival_by(
        chrono::Utc::now() + chrono::Duration::seconds(1800),
    );
    let response_future = client.travel_time(start_coord, end_coord, time_info);
    let response: i32 = core.run(response_future).unwrap();
    assert_that(&response).is_equal_to(123);
}

#[test]
fn test_returns_error_if_bad_request() {
    let _m = mock(
        "GET",
        mockito::Matcher::Regex(r"^/traveltime/\?.*$".to_string()),
    ).with_status(400)
        .with_header("Content-Type", "application/json")
        .with_body(
            "{\"error_message\": \"some error message\", \"error_code\": \"request-format\"}",
        )
        .create();
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = create_client(&handle);
    let start_coord = (51.525246, 0.084672);
    let end_coord = (51.559098, 0.074503);
    let response_future = client.travel_time(start_coord, end_coord, None);
    let response_error = core.run(response_future).unwrap_err();
    match response_error {
        citymapper::errors::Error(citymapper::errors::ErrorKind::BadRequestError(msg), _) => {
            assert_that(&msg).is_equal_to("some error message".to_string())
        }
        _ => panic!("Incorrect error type"),
    }
}


#[test]
fn test_other_status_codes_return_bad_response() {
    let _m = mock(
        "GET",
        mockito::Matcher::Regex(r"^/traveltime/\?.*$".to_string()),
    ).with_status(500)
        .with_header("Content-Type", "application/json")
        .with_body(
            "{\"error_message\": \"some error message\", \"error_code\": \"request-format\"}",
        )
        .create();
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = create_client(&handle);
    let start_coord = (51.525246, 0.084672);
    let end_coord = (51.559098, 0.074503);
    let response_future = client.travel_time(start_coord, end_coord, None);
    let response_error = core.run(response_future).unwrap_err();
    match response_error {
        citymapper::errors::Error(citymapper::errors::ErrorKind::BadResponse, _) => {}
        _ => panic!("Incorrect error type"),
    }
}

#[test]
fn test_returns_points_covered_for_single_coord() {
    let response_body = r#"{
        "points": [
            {
                "covered": true,
                "coord": [
                    51.578973,
                    -0.124147
                ]
            }
        ]
    }"#;
    let _m = mock(
        "GET",
        mockito::Matcher::Regex(
            r"^/singlepointcoverage/\?coord=123%2C456&key=123456".to_string(),
        ),
    ).with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(response_body)
        .create();
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = create_client(&handle);
    let coord: (f64, f64) = (123.0, 456.0);
    let response_future = client.single_point_coverage(coord);
    let response = core.run(response_future).unwrap();
    assert_that(&response.covered).is_true();
    assert_that(&response.coord).is_equal_to((51.578973, -0.124147));
}

#[test]
fn test_returns_points_covered_for_multiple_coords() {
    let expected_request_body =
        r#"{"points":[{"id":"test1","coord":[40.1,-73.0]},{"coord":[40.1,-73.0]}]}"#;
    let response_body = r#"{
        "points": [
            {
                "covered": true,
                "coord": [
                    51.578973,
                    -0.124147
                ],
                "id": "test1"
            }
        ]
    }"#;
    let _m = mock("POST", "/coverage/?key=123456")
        .match_body(expected_request_body)
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(response_body)
        .create();
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let client = create_client(&handle);
    let coord: (f64, f64) = (40.1, -73.0);
    let query1 = citymapper::MultiPointCoverageQuery::new(coord, "test1".to_string());
    let query2 = citymapper::MultiPointCoverageQuery::new(coord, None);
    let response_future = client.coverage(vec![query1, query2]);
    let response = core.run(response_future).unwrap();
    assert_that(&response.len()).is_equal_to(1);
    assert_that(&response[0].covered).is_true();
    assert_that(&response[0].coord).is_equal_to((51.578973, -0.124147));
    assert_that(&response[0].id).is_equal_to(Some("test1".to_string()));
}
