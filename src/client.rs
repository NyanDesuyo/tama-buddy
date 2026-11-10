use crate::{
    api::{CareRequest, ErrorResponse, HatchRequest, Snapshot},
    pet::Action,
};
use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, Response};
use std::time::Duration;

pub struct BackendClient {
    http: Client,
    base: String,
}
impl BackendClient {
    pub fn new(base: &str) -> Result<Self> {
        let url = reqwest::Url::parse(base).context("Invalid backend URL")?;
        if url.scheme() != "http"
            || url.host_str().is_none()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            bail!(
                "Use a backend URL such as http://127.0.0.1:3000, without a path or credentials."
            );
        }
        Ok(Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(1))
                .timeout(Duration::from_secs(3))
                .no_proxy()
                .build()?,
            base: base.trim_end_matches('/').into(),
        })
    }
    fn decode(&self, response: reqwest::Result<Response>) -> Result<Snapshot> {
        let response = response.with_context(|| {
            format!(
                "Cannot reach backend at {}. Start it with cargo run -- --server.",
                self.base
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let error: ErrorResponse = response
                .json()
                .context("Backend returned an invalid error response")?;
            bail!("Backend {status}: {}", error.error);
        }
        response
            .json()
            .context("Backend returned an invalid pet response")
    }
    pub fn refresh(&self) -> Result<Snapshot> {
        self.decode(self.http.get(format!("{}/api/pet", self.base)).send())
    }
    pub fn hatch(&self, name: String) -> Result<Snapshot> {
        self.decode(
            self.http
                .post(format!("{}/api/pet", self.base))
                .json(&HatchRequest { name })
                .send(),
        )
    }
    pub fn care(&self, action: Action) -> Result<Snapshot> {
        self.decode(
            self.http
                .post(format!("{}/api/pet/actions", self.base))
                .json(&CareRequest { action })
                .send(),
        )
    }
}
