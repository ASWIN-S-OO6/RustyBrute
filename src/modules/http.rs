use anyhow::{Context, Result};
use crate::cli::HttpAuth;
use reqwest::StatusCode;
use std::time::Duration;

/// Attempt an HTTP login. Returns `Ok(Some(msg))` on a suspected success.
pub async fn try_login(
    url: &str,
    user: &str,
    pass: &str,
    auth: HttpAuth,
    user_field: &str,
    password_field: &str,
    extra_fields: &[(String, String)],
    success_text: Option<&str>,
    fail_pattern: Option<&str>,
    success_code: u16,
    follow_redirects: bool,
    timeout_secs: u64,
) -> Result<Option<String>> {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if follow_redirects {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .context("failed to build HTTP client")?;

    let result = match auth {
        HttpAuth::Basic => {
            let resp = client
                .get(url)
                .basic_auth(user, Some(pass))
                .send()
                .await
                .with_context(|| format!("HTTP GET {url}"))?;
            evaluate_basic(resp, success_code).await
        }
        HttpAuth::Form => {
            let mut body: Vec<(String, String)> = Vec::new();
            body.push((user_field.to_string(), user.to_string()));
            body.push((password_field.to_string(), pass.to_string()));
            body.extend(extra_fields.iter().cloned());

            let resp = client
                .post(url)
                .form(&body)
                .send()
                .await
                .with_context(|| format!("HTTP POST {url}"))?;
            evaluate_form(resp, success_text, fail_pattern, success_code).await
        }
    };

    if result? {
        Ok(Some(format!("HTTP {user}:{pass} @ {url}")))
    } else {
        Ok(None)
    }
}

async fn evaluate_basic(resp: reqwest::Response, success_code: u16) -> Result<bool> {
    if resp.status() == StatusCode::from_u16(success_code)? {
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn evaluate_form(
    resp: reqwest::Response,
    success_text: Option<&str>,
    fail_pattern: Option<&str>,
    success_code: u16,
) -> Result<bool> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    // If a fail pattern was provided, its presence means failure.
    if let Some(fail) = fail_pattern {
        if body.contains(fail) {
            return Ok(false);
        }
    }

    // If a success text was provided, its presence means success.
    if let Some(success) = success_text {
        return Ok(body.contains(success));
    }

    // Fallback: match on status code.
    Ok(status == StatusCode::from_u16(success_code)?)
}