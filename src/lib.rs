//! A library for using the CityMapper API
//!
//! This library wraps the citymapper in a futures aware rust interface
//!
//! E.g
//!
//! ```rust,no_run
//! extern crate chrono;
//! extern crate tokio_core;
//! extern crate citymapper;
//! use tokio_core::reactor::Core;
//!
//! fn main() {
//!     let api_key = "<your api key>".to_string();
//!     let start_coord = (51.525246, 0.084672);
//!     let end_coord = (51.559098, 0.074503);
//!     let mut core = Core::new().unwrap();
//!     let handle = core.handle();
//!     let client = citymapper::ClientBuilder::new(&handle, api_key).build();
//!     let time_info = citymapper::TimeConstraint::arrival_by(
//!         chrono::Utc::now() + chrono::Duration::seconds(1800),
//!     );
//!     let response_future = client.travel_time(start_coord, end_coord, time_info);
//!     let response = core.run(response_future).unwrap();
//!     println!("Response: {:?}", response);
//! }
//! ```
//!
//! As you can see you first need to instantiate a `ClientBuilder` and use
//! that to create an instance of `Client`.
extern crate chrono;
extern crate tokio_core;
extern crate hyper;
extern crate hyper_tls;
extern crate url;
extern crate futures;
#[macro_use]
extern crate error_chain;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate slog;
extern crate slog_stdlog;

use hyper::client::{Client as HyperClient, FutureResponse};
use hyper::{Method, Request, Chunk, StatusCode};
use hyper::header::{ContentType, ContentLength};
use futures::{Future, Stream};
use slog::Drain;

use tokio_core::reactor::Handle;

/// How the CityMapper API should treat the `time` argument. Currently the
/// only option the API provides is `Arrival`.
enum TimeType {
    Arrival,
}

/// The citymapper travel time API accepts an optional time argument which specifies
/// when you want the travel time to be calculated for. This struct represents
/// that argument.
pub struct TimeConstraint {
    time: chrono::DateTime<chrono::Utc>,
    time_type: TimeType,
}

impl TimeConstraint {
    /// Returns a new TimeConstraint
    ///
    /// # Arguments
    ///
    /// * `time` - The time to calculate the travel time with respect to
    pub fn arrival_by(time: chrono::DateTime<chrono::Utc>) -> TimeConstraint {
        return TimeConstraint {
            time: time,
            time_type: TimeType::Arrival,
        };
    }
}

/// A WGS84 coordinate in the form (latitude, longitude)
type Coord = (f64, f64);

/// Interface to the CityMapper API
pub struct Client {
    handle: Box<Handle>,
    api_key: String,
    base_url: url::Url,
    logger: slog::Logger,
}

/// Interface for building a citymapper client
pub struct ClientBuilder<'a> {
    handle: &'a Handle,
    api_key: String,
    base_url: Option<url::Url>,
    logger: Option<slog::Logger>,
}

impl<'a> ClientBuilder<'a> {
    /// Create a `ClientBuilder` which will initialize the client with the
    /// given tokio handle and the api key.
    pub fn new(handle: &'a Handle, api_key: String) -> ClientBuilder {
        return ClientBuilder {
            handle: handle,
            api_key: api_key,
            base_url: None,
            logger: None,
        };
    }

    /// Set the base URL of the client to something other than the default,
    /// only really useful for testing.
    pub fn with_base_url(&'a mut self, base_url: url::Url) -> &'a mut ClientBuilder {
        self.base_url = Some(base_url);
        self
    }

    /// Set the logger to use, if not set the default `log` compatible logger will be used
    pub fn with_logger(&'a mut self, logger: slog::Logger) -> &'a mut ClientBuilder {
        self.logger = Some(logger);
        self
    }

    /// Create a citymapper client from the configuration this builder represents.
    pub fn build(&self) -> Client {
        let base_url = match self.base_url {
            Some(ref url) => url.clone(),
            None => url::Url::parse("https://developer.citymapper.com/api/1").unwrap(),
        };
        let logger = match self.logger {
            Some(ref logger) => logger.clone(),
            None => slog::Logger::root(slog_stdlog::StdLog.fuse(), o!()),
        };
        let handle = Box::new(self.handle.clone());
        return Client {
            handle: handle,
            api_key: self.api_key.clone(),
            base_url: base_url.clone(),
            logger: logger,
        };
    }
}

pub mod errors {
    error_chain!{
        errors {
            BadRequestError(message: String)
                BadResponse
        }
        foreign_links {
            BadJsonResponse(::serde_json::Error);
            NetworkError(::hyper::Error);
        }
    }
}

#[derive(Deserialize, Debug)]
struct TimeTravelledResponse {
    travel_time_minutes: i32,
}


/// One point in a response from the coverage API (either single or multi point)
#[derive(Deserialize, Debug, Clone)]
pub struct PointCoverage {
    /// Whether or not the API covers this point
    pub covered: bool,
    /// The coordinate that was passed to the API
    pub coord: (f64, f64),
    /// The ID that was passed to the API, if any. See the `MultiPointCoverageQuery` struct.
    pub id: Option<String>,
}

#[derive(Deserialize)]
struct PointCoverageResponse {
    points: Vec<PointCoverage>,
}

#[derive(Deserialize)]
struct BadRequestResponse {
    error_message: String,
}

/// An individual point to send to the multi point coverage query API
#[derive(Serialize)]
pub struct MultiPointCoverageQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    coord: Coord,
}

#[derive(Serialize)]
struct MultiPointCoverageRequestBody {
    points: Vec<MultiPointCoverageQuery>,
}

impl MultiPointCoverageQuery {
    /// Create a new point to query against. The `id` argument can be `None`
    /// but if it is a string it will be passed to the API and returned in the
    /// response, see the citymapper API docs for more details.
    ///
    /// ```rust
    /// # use citymapper::MultiPointCoverageQuery;
    /// let point_without_id = MultiPointCoverageQuery::new((0.12, 3.45), None);
    /// let point_with_id = MultiPointCoverageQuery::new((0.12, 3.45), "someid".to_string());
    /// ```
    pub fn new<T: Into<Option<String>>>(coord: Coord, id: T) -> MultiPointCoverageQuery {
        MultiPointCoverageQuery {
            id: id.into(),
            coord: coord,
        }
    }
}


/// The main interface for making calls to CityMapper
impl Client {
    /// Returns a future containing the travel time from start coord to end coord as per the
    /// citymapper api documented at https://citymapper.3scale.net/docs
    ///
    /// # Arguments
    ///
    /// * `start_coord` - The (latitude, longitude) pair to start from
    /// * `end_coord` - The (latitude, longitude) pair to start from
    /// * `time_constraint` - If specified is an instance of `TimeConstraint` specifying time
    /// constraints on the calculation of the travel time.
    ///
    /// # Returns
    /// A future which will resolve to the number of minutes to travel.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # extern crate chrono;
    /// # extern crate tokio_core;
    /// # extern crate citymapper;
    /// # use tokio_core::reactor::Core;
    ///
    /// # fn main() {
    /// # let core = Core::new().unwrap();
    /// # let handle = core.handle();
    /// # let api_key = "some api key".to_string();
    /// # let client = citymapper::ClientBuilder::new(&handle, api_key).build();
    /// let start_coord = (51.525246, 0.084672);
    /// let end_coord = (51.559098, 0.074503);
    /// let response_without_time_constraint = client.travel_time(start_coord, end_coord, None);
    ///
    /// let time_info = citymapper::TimeConstraint::arrival_by(
    ///     chrono::Utc::now() + chrono::Duration::seconds(1800),
    /// );
    /// let response_with_time_constraint = client.travel_time(start_coord, end_coord, time_info);
    /// # }
    /// ```
    pub fn travel_time<T: Into<Option<TimeConstraint>>>(
        &self,
        start_coord: Coord,
        end_coord: Coord,
        time_constraint: T,
    ) -> Box<Future<Item = i32, Error = errors::Error>> {
        let mut params: Vec<(String, String)> =
            vec![
                (
                    "startcoord".to_string(),
                    format!("{0},{1}", start_coord.0, start_coord.1)
                ),
                (
                    "endcoord".to_string(),
                    format!("{0},{1}", end_coord.0, end_coord.1)
                ),
            ];
        if let Some(constraint) = time_constraint.into() {
            params.push(("time".to_string(), format!("{0}", constraint.time)));
            match constraint.time_type {
                TimeType::Arrival => params.push(("time_type".to_string(), "arrival".to_string())),
            }
        };
        let request = self.build_request("traveltime", params, None);
        self.make_request(request, |body| {
            let result: TimeTravelledResponse = serde_json::from_slice(&body)?;
            Ok(result.travel_time_minutes)
        })
    }

    /// Check whether a single (latitude, longitude) pair is covered by citymapper
    ///
    /// # Arguments
    ///
    /// * coord - The (latitiude, longitude) pair to check for coverage
    ///
    /// # Returns
    ///
    /// A future which resolves to a `PointCoverage` struct.
    ///
    /// # Example
    /// ```rust,no_run
    /// # extern crate tokio_core;
    /// # extern crate citymapper;
    /// # use tokio_core::reactor::Core;
    ///
    /// # fn main() {
    /// # let core = Core::new().unwrap();
    /// # let handle = core.handle();
    /// # let api_key = "some api key".to_string();
    /// # let client = citymapper::ClientBuilder::new(&handle, api_key).build();
    /// let coord = (51.525246, 0.084672);
    ///
    /// let response = client.single_point_coverage(coord);
    /// # }
    /// ```
    pub fn single_point_coverage(
        &self,
        point: Coord,
    ) -> Box<Future<Item = PointCoverage, Error = errors::Error>> {
        let params = vec![("coord".to_string(), format!("{0},{1}", point.0, point.1))];
        let request = self.build_request("singlepointcoverage", params, None);
        self.make_request(request, |body| {
            let result: PointCoverageResponse = serde_json::from_slice(&body)?;
            if result.points.len() != 1 {
                return Err(errors::ErrorKind::BadResponse.into());
            }
            Ok(result.points[0].clone())
        })
    }

    /// Check whether multiple (latitude, longitude) pairs are covered by citymapper
    ///
    /// # Arguments
    /// * points - A set of `MultiPointCoverageQuery` to check for coverage
    ///
    /// # Returns
    ///
    /// A future which resolves to a `Vec<PointCoverage>`
    ///
    /// # Example
    /// ```rust,no_run
    /// # extern crate tokio_core;
    /// # extern crate citymapper;
    /// # use tokio_core::reactor::Core;
    /// # use citymapper::MultiPointCoverageQuery;
    ///
    /// # fn main() {
    /// # let core = Core::new().unwrap();
    /// # let handle = core.handle();
    /// # let api_key = "some api key".to_string();
    /// # let client = citymapper::ClientBuilder::new(&handle, api_key).build();
    /// let coord = (51.525246, 0.084672);
    /// let point1 = MultiPointCoverageQuery::new(coord, "someid".to_string());
    /// let coord2 = (52.345, 0.0745);
    /// let point2 = MultiPointCoverageQuery::new(coord2, None);
    ///
    /// let response = client.coverage(vec![point1, point2]);
    /// # }
    /// ```
    pub fn coverage(
        &self,
        points: Vec<MultiPointCoverageQuery>,
    ) -> Box<Future<Item = Vec<PointCoverage>, Error = errors::Error>> {
        let req_body = serde_json::to_string(&MultiPointCoverageRequestBody { points: points })
            .unwrap();
        let request = self.build_request("coverage", Vec::new(), req_body);
        self.make_request(request, |body| {
            let result: PointCoverageResponse = serde_json::from_slice(&body)?;
            Ok(result.points)
        })
    }

    fn build_request<T: Into<Option<String>>>(
        &self,
        path: &str,
        params: Vec<(String, String)>,
        body: T,
    ) -> Request {
        let body = body.into();
        let mut req_url = self.base_url.clone();
        {
            let mut path_segments = req_url.path_segments_mut().unwrap();
            path_segments.push(path);
            path_segments.push("");
        }
        for (param, value) in params {
            req_url.query_pairs_mut().append_pair(&param, &value);
        }
        req_url.query_pairs_mut().append_pair("key", &self.api_key);
        let req_uri: hyper::Uri = req_url.clone().into_string().parse().unwrap();
        let redacted_url = self.redacted_url(req_url);
        info!(
            self.logger,
            "Making citymapper request to {0}",
            redacted_url
        );

        let method = if body.is_some() {
            Method::Post
        } else {
            Method::Get
        };
        let mut request = Request::new(method, req_uri);
        request.headers_mut().set(ContentType::json());

        if let Some(body) = body {
            debug!(self.logger, "Request body is {0}", body);
            request.headers_mut().set(ContentLength(body.len() as u64));
            request.set_body(body);
        } else {
            request.headers_mut().set(ContentLength(0));
        }
        request
    }

    fn redacted_url(&self, url: url::Url) -> url::Url {
        let mut redacted_params: Vec<(String, String)> = Vec::new();
        for (param, value) in url.query_pairs() {
            if param == "key" {
                redacted_params.push(("key".to_string(), "redacted".to_string()));
            } else {
                redacted_params.push((param.to_string(), value.to_string()));
            }
        }
        let mut result = url.clone();
        result.query_pairs_mut().clear();
        for (param, value) in redacted_params {
            result.query_pairs_mut().append_pair(&param, &value);
        }
        result
    }

    fn make_request<T: 'static, F: 'static>(
        &self,
        request: Request,
        response_handler: F,
    ) -> Box<Future<Item = T, Error = errors::Error>>
    where
        F: Fn(Chunk) -> Result<T, errors::Error>,
    {
        let response = match request.uri().scheme() {
            Some("http") => self.make_http_request(request),
            Some("https") => self.make_https_request(request),
            _ => panic!("Unknown scheme in base URL"),
        };

        let err_logger = self.logger.clone();
        let future_logger = self.logger.clone();
        let future = response
            .map_err(move |e| -> errors::Error {
                error!(err_logger, "Error fetching from citymapper servers: {0}", e);
                e.into()
            })
            .and_then(move |response| {
                debug!(future_logger, "{0} received", response.status());
                let status = response.status().clone();
                response
                    .body()
                    .concat2()
                    .map_err(|e| -> errors::Error { e.into() })
                    .and_then(move |body: Chunk| {
                        match status {
                            StatusCode::BadRequest => {}
                            StatusCode::Ok => {}
                            _ => return Err(errors::ErrorKind::BadResponse.into()),
                        };
                        debug!(
                            future_logger,
                            "Response content: {:?}",
                            std::str::from_utf8(&body).unwrap_or("Invalid UTF8 content")
                        );
                        if status == StatusCode::BadRequest {
                            let error_report: BadRequestResponse = serde_json::from_slice(&body)?;
                            let e = Err(
                                errors::ErrorKind::BadRequestError(error_report.error_message)
                                    .into(),
                            );
                            return e;
                        }
                        Ok(response_handler(body)?)
                    })
            });
        Box::new(future)
    }

    fn make_http_request(&self, request: Request) -> FutureResponse {
        let http_client = HyperClient::new(&self.handle);
        http_client.request(request)
    }

    fn make_https_request(&self, request: Request) -> FutureResponse {
        let http_client = HyperClient::configure()
            .connector(hyper_tls::HttpsConnector::new(4, &self.handle).unwrap())
            .build(&self.handle);
        http_client.request(request)
    }
}
