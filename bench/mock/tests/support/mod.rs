use reqwest::{Client, Method, RequestBuilder};
use vc_mock_server::{MockHandle, MockServer};

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A server of its own plus a client for it.
pub struct Mock {
    pub handle: MockHandle,
    pub client: Client,
}

impl Mock {
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { handle: MockServer::start().await?, client: Client::builder().build()? })
    }

    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client.request(method, format!("{}{path}", self.handle.base_url()))
    }

    pub fn get(&self, path: &str) -> RequestBuilder {
        self.request(Method::GET, path)
    }
}
