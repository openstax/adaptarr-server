//! Client for testing interactions with a server.

use adaptarr::{
    api::{State, new_app, pages},
    config::{self, Config},
    i18n::I18n,
    mail::Mailer,
};
use actix_web::{
    Body,
    HttpMessage,
    client::{ClientRequest, ClientRequestBuilder, ClientResponse},
    error::JsonPayloadError,
    http::{
        Cookie,
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
use tempfile::TempDir;

use super::{
    db::Pool,
    mock::Mocker,
    session::Session,
    support::{Fixture, TestOptions},
};

lazy_static! {
    static ref TEMP_DIR: TempDir = TempDir::new()
        .expect("Cannot create temporary directory");

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
            path: TEMP_DIR.path().to_path_buf(),
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
    server: TestServer,
    /// Current session.
    session: Option<Cookie<'static>>,
}

impl Client {
    /// Build a new test client driver.
    pub fn new(db: Pool) -> Client {
        let state = State {
            config: CONFIG.clone(),
            db,
            mailer: Mailer::from_config(CONFIG.mail.clone()).unwrap(),
            events: Mocker::new().addr(),
            i18n: I18n::load().unwrap(),
            importer: Mocker::new().addr(),
        };

        let server = TestServer::with_factory(move || vec![
            new_app(state.clone()),
            pages::app(state.clone()),
        ]);

        Client {
            server,
            session: None,
        }
    }

    /// Set a session cookie to be used in all requests.
    ///
    /// Passing `None` will remove the cookie.
    pub fn set_session(&mut self, session: Option<Cookie<'static>>) {
        self.session = session;
    }

    /// Prepare a request.
    pub fn request(&mut self, method: Method, path: &str) -> Request {
        let mut request = self.server.client(method, path);

        if let Some(ref cookie) = self.session {
            request.cookie(cookie.clone());
        }

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
    #[allow(dead_code)]
    pub fn put(&mut self, path: &str) -> Request {
        self.request(Method::PUT, path)
    }

    /// Prepare a DELETE request.
    #[allow(dead_code)]
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
        let mut client = Client::new(opts.pool.clone());

        if let Some(session) = opts.get::<Session>() {
            client.set_session(Some(session.cookie()));
        }

        Ok(client)
    }
}

/// A prepared but not yet sent request.
pub struct Request<'client> {
    client: &'client mut Client,
    request: ClientRequestBuilder,
}

impl<'client> Request<'client> {
    /// Add a cookie.
    #[allow(dead_code)]
    pub fn cookie(mut self, cookie: Cookie) -> Self {
        self.request.cookie(cookie);
        self
    }

    /// Send this request with specified body and content-type, returning the
    /// response, and blocking while waiting.
    ///
    /// This function will panic on errors.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn assert_status(self, code: StatusCode) -> Self {
        let status = self.response.status();
        assert_eq!(status, code, "Bad status code");
        self
    }

    /// Assert that this response is a success.
    pub fn assert_success(mut self) -> Self {
        let Response { ref mut client, ref response } = self;
        let status = response.status();

        if !status.is_success() {
            if let Ok(err) = client.server.execute(response.json::<ErrorData>()) {
                panic!("Expected success, not {} {}: {}",
                    status, err.error, err.raw);
            } else {
                let body = self.body();
                let err = String::from_utf8_lossy(&body);
                panic!("Expected success, not {}: {}", status, err);
            }
        }

        self
    }

    /// Assert that this response is a redirection
    #[allow(dead_code)]
    pub fn assert_redirection(mut self) -> Self {
        let Response { ref mut client, ref response } = self;
        let status = response.status();

        if !status.is_redirection() {
            if let Ok(err) = client.server.execute(response.json::<ErrorData>()) {
                panic!("Expected redirection, not {} {}: {}",
                    status, err.error, err.raw);
            } else {
                let body = self.body();
                let err = String::from_utf8_lossy(&body);
                panic!("Expected redirection, not {}: {}", status, err);
            }
        }

        self
    }

    /// Assert that this response is an API error with specified HTTP status
    /// code and error code string.
    #[allow(dead_code)]
    pub fn assert_error(self, status: StatusCode, code: &str) {
        let s = self.response.status();

        if s.is_success() || s.is_redirection() {
            panic!("Expected {} {}, not {}", status, code, s);
        }

        let data: ErrorData = match self.client.server.execute(self.response.json()) {
            Ok(data) => data,
            Err(_) => {
                let body = self.body();
                let data = String::from_utf8_lossy(&body);
                panic!("Expected {} {}, not {} {}", status, code, s, data);
            }
        };

        if status != status {
            panic!("Expected {} {}, not {} {} {}",
                status, code, s, data.error, data.raw);
        }

        assert_eq!(data.error, code);
    }

    /// Get value of a header.
    ///
    /// This function will panic if header was not set.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
