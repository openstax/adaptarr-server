//! Client for testing interactions with a server.

use adaptarr::{
    api::{State, new_app, pages},
    config::{self, Config},
    events::EventManager,
    i18n::I18n,
    import::Importer,
    mail::Mailer,
    processing::TargetProcessor,
};
use actix_web::{
    Body,
    HttpMessage,
    client::{ClientRequest, ClientRequestBuilder, ClientResponse},
    error::JsonPayloadError,
    http::{
        Cookie,
        HttpTryFrom,
        Method,
        StatusCode,
        header::{AsHeaderName, HeaderValue},
    },
    test::TestServer,
};
use bytes::Bytes;
use failure::Error;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{fmt::Display, net::Ipv4Addr};

use super::{db::Pool, mock::Mocker, support::{Fixture, TestOptions}};

lazy_static! {
    pub static ref CONFIG: Config = Config {
        server: config::Server {
            address: (Ipv4Addr::LOCALHOST, 80).into(),
            domain: "adaptarr.test".to_string(),
            secret: vec![0; 32],
        },
        database: None,
        mail: adaptarr::mail::Config {
            sender: "Adaptarr! <noreply@adaptarr.test>".parse().unwrap(),
            transport: adaptarr::mail::Transports::Log,
        },
        storage: config::Storage {
            path: "tests/.run-data".into(),
        },
        logging: config::Logging {
            level: log::LevelFilter::Debug,
            network: None,
            filters: Default::default(),
        },
        sentry: None,
    };
}

pub struct Client {
    events: Mocker<EventManager>,
    importer: Mocker<Importer>,
    xref_processor: Mocker<TargetProcessor>,
    server: TestServer,
}

impl Client {
    /// Build a new test client driver.
    pub fn new(db: Pool) -> Client {
        let events = Mocker::new();
        let importer = Mocker::new();
        let xref_processor = Mocker::new();

        let state = State {
            config: CONFIG.clone(),
            db,
            mailer: Mailer::from_config(CONFIG.mail.clone()).unwrap(),
            events: events.addr(),
            i18n: I18n::load().unwrap(),
            importer: importer.addr(),
            xref_processor: xref_processor.addr(),
        };

        let server = TestServer::with_factory(move || vec![
            new_app(state.clone()),
            pages::app(state.clone()),
        ]);

        Client {
            events,
            importer,
            xref_processor,
            server,
        }
    }

    /// Prepare a request.
    pub fn request(&mut self, method: Method, path: &str) -> Request {
        let request = self.server.client(method, path);

        Request {
            client: self,
            request,
        }
    }

    /// Prepare a GET request.
    pub fn get(&mut self, path: &str) -> Request {
        self.request(Method::GET, path)
    }

    /// Prepare a POST request.
    pub fn post(&mut self, path: &str) -> Request {
        self.request(Method::POST, path)
    }

    /// Prepare a PUT request.
    pub fn put(&mut self, path: &str) -> Request {
        self.request(Method::PUT, path)
    }

    /// Prepare a DELETE request.
    pub fn delete(&mut self, path: &str) -> Request {
        self.request(Method::DELETE, path)
    }

    fn execute(&mut self, request: ClientRequest) -> Response {
        let response = self.server.execute(request.send())
            .expect("Request should execute successfully");
        Response {
            client: self,
            response,
        }
    }
}

impl Fixture for Client {
    fn make(opts: &TestOptions) -> Result<Client, Error> {
        Ok(Client::new(opts.pool.clone()))
    }
}

/// A prepared but not yet sent request.
pub struct Request<'client> {
    client: &'client mut Client,
    request: ClientRequestBuilder,
}

impl<'client> Request<'client> {
    /// Add a cookie.
    pub fn cookie(mut self, cookie: Cookie) -> Self {
        self.request.cookie(cookie);
        self
    }

    /// Send this request with specified body and content-type, returning the
    /// response, and blocking while waiting.
    ///
    /// This function will panic on errors.
    pub fn body<B>(self, body: B) -> Response<'client>
    where
        Body: From<B>,
    {
        let Request { client, mut request } = self;
        client.execute(request.body(body).expect("Request should build successfully"))
    }

    /// Send this request with a urlencoded form as its body, returning the
    /// response, and blocking while waiting.
    ///
    /// This function will panic on errors.
    pub fn form<T>(self, form: T) -> Response<'client>
    where
        T: Serialize,
    {
        let Request { client, mut request } = self;
        client.execute(request.form(form).expect("Request should build successfully"))
    }

    /// Send this request with JSON as its body, returning the response, and
    /// blocking while waiting.
    ///
    /// This function will panic on errors.
    pub fn json<T>(self, json: T) -> Response<'client>
    where
        T: Serialize,
    {
        let Request { client, mut request } = self;
        client.execute(request.json(json).expect("Request should build successfully"))
    }

    /// Send this request and return the response, blocking while waiting.
    ///
    /// This function will panic on errors.
    pub fn send(self) -> Response<'client> {
        let Request { client, mut request } = self;
        client.execute(request.finish().expect("Request should build successfully"))
    }
}

pub struct Response<'client> {
    client: &'client mut Client,
    response: ClientResponse,
}

impl<'client> Response<'client> {
    /// Assert that this response uses specified code.
    pub fn assert_status(self, code: StatusCode) -> Self {
        let status = self.response.status();
        assert_eq!(status, code, "Bad status code");
        self
    }

    /// Assert that this response is a success.
    pub fn assert_success(self) -> Self {
        let status = self.response.status();
        assert!(status.is_success(), "Expected success, not {}", status);
        self
    }

    /// Assert that this response is a redirection
    pub fn assert_redirection(self) -> Self {
        let status = self.response.status();
        assert!(status.is_redirection(), "Expected redirection, not {}", status);
        self
    }

    /// Assert that this response is an API error with specified HTTP status
    /// code and error code string.
    pub fn assert_error(self, status: StatusCode, code: &str) {
        assert_eq!(self.response.status(), status);

        let data: ErrorData = self.json();
        assert_eq!(data.error, code);
    }

    /// Get value of a header.
    ///
    /// This function will panic if header was not set.
    pub fn header<N>(&self, name: N) -> &HeaderValue
    where
        N: AsHeaderName + Clone + Display,
    {
        match self.response.headers().get(name.clone()) {
            Some(h) => h,
            None => panic!("Expected header {} to be set", name),
        }
    }

    /// Get value of a cookie.
    ///
    /// This function will panic if cookie was not set.
    pub fn cookie(&self, name: &str) -> Cookie {
        match self.response.cookie(name) {
            Some(c) => c,
            None => panic!("Expected {} cookie to be set", name),
        }
    }

    /// Read raw body of this response.
    ///
    /// This function will panic on errors.
    pub fn body(self) -> Bytes {
        let Response { client, response } = self;
        client.server.execute(response.body()).unwrap()
    }

    /// Read this response as JSON and deserialize it.
    ///
    /// This function will panic on errors.
    pub fn json<T>(self) -> T
    where
        T: DeserializeOwned + 'static,
    {
        let Response { client, response } = self;

        match client.server.execute(response.json()) {
            Ok(t) => t,
            Err(JsonPayloadError::ContentType) => panic!(
                r#"Bad Content-Type: {:?}, expected "application/json""#,
                response.content_type(),
            ),
            Err(err) => panic!("{}", err),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ErrorData {
    error: String,
    raw: String,
}
