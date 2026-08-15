//! Direct HTTPS client for the Simyo API (`appapi.simyo.nl`).
//! No proxy, no third party: credentials go straight from this machine to Simyo.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, USER_AGENT};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::debug;

use crate::flow;

pub const BASE_URL_V1: &str = "https://appapi.simyo.nl/webapi/api/v1";
pub const BASE_URL_V2: &str = "https://appapi.simyo.nl/webapi/api/v2";

/// Recovered from the official iOS app `MijnSimyoFT` (see eSIM-Tools
/// `client-identity.js`). The API requires these for every request.
const CLIENT_TOKEN: &str = "e77b7e2f43db41bb95b17a2a11581a38";
const CLIENT_VERSION: &str = "4.28.0";
/// Note the double space between version and `(iOS ...)` — required.
const USER_AGENT_VALUE: &str = "MijnSimyoFT/4.28.0  (iOS 18.2; iPhone12,8)";

pub struct ApiClient {
    http: Client,
    device_id: String,
}

impl ApiClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;
        let device_id = uuid::Uuid::new_v4().to_string().to_uppercase();
        Ok(Self { http, device_id })
    }

    fn headers(&self, session: Option<&str>) -> Result<HeaderMap> {
        let mut h = HeaderMap::new();
        h.insert("X-Client-Token", HeaderValue::from_static(CLIENT_TOKEN));
        h.insert("X-Client-Platform", HeaderValue::from_static("ios"));
        h.insert("X-Client-Version", HeaderValue::from_static(CLIENT_VERSION));
        h.insert("X-Device-ID", HeaderValue::from_str(&self.device_id)?);
        h.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(ACCEPT, HeaderValue::from_static("application/json"));
        if let Some(token) = session {
            h.insert("X-Session-Token", HeaderValue::from_str(token)?);
        }
        Ok(h)
    }

    fn send(
        &self,
        method: Method,
        url: &str,
        session: Option<&str>,
        body: Option<&Value>,
    ) -> Result<Value> {
        debug!("{method} {url}");
        let mut req = self
            .http
            .request(method, url)
            .headers(self.headers(session)?);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().context("HTTP request failed")?;
        parse_response(resp)
    }

    /// POST /sessions — password is never logged or persisted.
    pub fn login(&self, phone: &str, password: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V1}/sessions");
        self.send(
            Method::POST,
            &url,
            None,
            Some(&json!({ "phoneNumber": phone, "password": password })),
        )
    }

    /// POST /v2/security.verifyOTP with the temporary session.
    pub fn verify_otp(&self, session: &str, code: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V2}/security.verifyOTP");
        self.send(
            Method::POST,
            &url,
            Some(session),
            Some(&json!({ "rememberMe": true, "token": code })),
        )
    }

    /// GET /settings/simcard — eSIM order status.
    pub fn get_simcard(&self, session: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V1}/settings/simcard");
        self.send(Method::GET, &url, Some(session), None)
    }

    /// POST /settings/simcard — request a device change (EMAIL validation).
    pub fn apply_new_esim(&self, session: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V1}/settings/simcard");
        self.send(
            Method::POST,
            &url,
            Some(session),
            Some(&json!({ "initialValidationMethod": "EMAIL", "esim": true })),
        )
    }

    /// POST /esim/verify-code — submit the email validation code.
    pub fn verify_code(&self, session: &str, code: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V1}/esim/verify-code");
        self.send(
            Method::POST,
            &url,
            Some(session),
            Some(&json!({ "validationCode": code })),
        )
    }

    /// GET /esim/get-by-customer — fetch activationCode / iccid / phoneNumber.
    pub fn get_esim(&self, session: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V1}/esim/get-by-customer");
        self.send(Method::GET, &url, Some(session), None)
    }

    /// POST /esim/reorder-profile-installed — confirm installation.
    pub fn confirm_install(&self, session: &str) -> Result<Value> {
        let url = format!("{BASE_URL_V1}/esim/reorder-profile-installed");
        self.send(Method::POST, &url, Some(session), None)
    }
}

fn parse_response(resp: Response) -> Result<Value> {
    let status = resp.status();
    let body: Value = resp.json().unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = flow::extract_error_message(&body).unwrap_or_else(|| format!("HTTP {status}"));
        bail!("Simyo API error (HTTP {status}): {msg}");
    }
    Ok(body)
}
