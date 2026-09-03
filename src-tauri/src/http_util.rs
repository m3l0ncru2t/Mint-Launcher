/// Checks the response status and, on failure, reads the body text so the
/// real reason (an API's error JSON, or a WAF block page) reaches the user
/// instead of a bare "403 Forbidden".
pub async fn ensure_success(resp: reqwest::Response, context: &str) -> anyhow::Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let body = body.trim();
    if body.is_empty() {
        anyhow::bail!("{context} failed: HTTP {status}");
    }
    anyhow::bail!("{context} failed: HTTP {status} - {body}");
}
