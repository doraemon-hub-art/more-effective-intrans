/*
 * @file traditional.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Provider for traditional dictionary and machine translation APIs
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::fmt::Write as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use md5::{Digest, Md5};
use serde::Deserialize;
use tracing::debug;

use crate::error::{Error, Result};
use crate::pipeline::translator::Translator;

// Per-file logger id: UpperCamelCase version of this file's path under src/, in brackets.
const LOG_ID: &str = "[PipelineProvidersTraditional]";

/// Credentials. They are read from the environment and never from a config file.
pub const APPID_ENV: &str = "BAIDU_TRANSLATE_APPID";
pub const SECRET_ENV: &str = "BAIDU_TRANSLATE_SECRET";

/// Endpoint override: a mirror, or a stand-in server used while developing without keys.
pub const ENDPOINT_ENV: &str = "BAIDU_TRANSLATE_ENDPOINT";

/// The public endpoint of the general text translation API. Note the host is
/// `api.fanyi.baidu.com` — the platform site lives on `fanyi-api.baidu.com`.
const DEFAULT_ENDPOINT: &str = "https://api.fanyi.baidu.com/api/trans/vip/translate";

/// Language codes as this provider spells them; the input is always a Chinese word here.
const FROM_LANG: &str = "zh";
const TO_LANG: &str = "en";

/// The provider wants the salt to be a number in 32768..=65536.
const SALT_MIN: u64 = 32768;
const SALT_SPAN: u64 = 65536 - SALT_MIN + 1;

/// Give up on one request after this long, so a wedged socket cannot hold a task forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The provider's own code for "success", used when it reports one with an empty result.
const SUCCESS_CODE: &str = "52000";

/// Baidu's general text translation API.
pub struct BaiduTranslator {
    client: reqwest::Client,
    appid: String,
    secret: String,
    endpoint: String,
}

impl BaiduTranslator {
    /// Reads both credentials from the environment and keeps the default endpoint.
    ///
    /// This is [`Self::from_settings`] with nothing filled in, for a run that has no settings
    /// to read.
    pub fn from_env() -> Result<Self> {
        Self::from_settings("", "", "")
    }

    /// Builds a translator from the settings, and takes whatever they leave empty from the
    /// environment variable of the same name.
    ///
    /// The configuration file is where the credentials belong; the variables stay as the
    /// fallback, so a session that has them exported keeps working as before.
    pub fn from_settings(appid: &str, secret: &str, endpoint: &str) -> Result<Self> {
        let appid = match appid {
            "" => required_env(APPID_ENV)?,
            value => value.to_owned(),
        };
        let secret = match secret {
            "" => required_env(SECRET_ENV)?,
            value => value.to_owned(),
        };
        let endpoint = match endpoint {
            "" => std::env::var(ENDPOINT_ENV).unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned()),
            value => value.to_owned(),
        };

        Self::with_endpoint(appid, secret, endpoint)
    }

    /// Builds a translator for an explicit endpoint: a mirror, or the stand-in server used
    /// to exercise the pipeline before real credentials exist.
    pub fn with_endpoint(appid: String, secret: String, endpoint: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(Error::from)?;

        Ok(Self {
            client,
            appid,
            secret,
            endpoint,
        })
    }

    /// Sends one query and returns the translated text.
    async fn request(&self, query: &str) -> Result<String> {
        let salt = salt_from_clock();
        let salt_text = salt.to_string();
        let sign = signature(&self.appid, query, &salt_text, &self.secret);

        // The shape the provider documents: parameters in the query string, form content
        // type. reqwest percent-encodes every value on its way out.
        let params = [
            ("appid", self.appid.as_str()),
            ("q", query),
            ("from", FROM_LANG),
            ("to", TO_LANG),
            ("salt", salt_text.as_str()),
            ("sign", sign.as_str()),
        ];

        debug!(target: LOG_ID, "Requesting a translation of {query:?} from {}.", self.endpoint);

        let response = self
            .client
            .post(&self.endpoint)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .query(&params)
            .send()
            .await
            .map_err(Error::from)?;

        let status = response.status();
        let body = response.text().await.map_err(Error::from)?;
        debug!(target: LOG_ID, "Provider answered {status} with {} byte(s).", body.len());

        parse_response(&body)
    }
}

impl Translator for BaiduTranslator {
    fn translate(&self, text: &str) -> impl Future<Output = Result<String>> + Send {
        self.request(text)
    }
}

/// Response of the general translation API: either translations, or a reason it refused.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    /// One entry per input line, present when the call succeeded.
    trans_result: Option<Vec<ApiTranslation>>,
    /// Present when the call was refused.
    error_code: Option<String>,
    error_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiTranslation {
    /// The translated text. The provider also sends `src`, which is of no use here.
    dst: String,
}

/// Pulls the translation out of a response body, or turns the provider's refusal into an
/// error. Kept apart from sending so it can be tested without a network.
fn parse_response(body: &str) -> Result<String> {
    let parsed: ApiResponse = serde_json::from_str(body)
        .map_err(|error| Error::UnexpectedResponse(format!("{error}; body was {body:.200}")))?;

    if let Some(translations) = parsed.trans_result {
        let lines: Vec<&str> = translations.iter().map(|item| item.dst.as_str()).collect();
        if !lines.is_empty() {
            // A multi-line query comes back as one entry per line.
            return Ok(lines.join("\n"));
        }
    }

    match parsed.error_code {
        Some(code) if code != SUCCESS_CODE => Err(Error::Provider {
            code,
            message: parsed.error_msg.unwrap_or_default(),
        }),
        _ => Err(Error::UnexpectedResponse(format!(
            "no translation in {body:.200}"
        ))),
    }
}

/// `sign = md5(appid + query + salt + secret)`, lowercase hex. The provider concatenates
/// the four parts with nothing in between, so the salt has to be spelled exactly as sent.
fn signature(appid: &str, query: &str, salt: &str, secret: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(appid.as_bytes());
    hasher.update(query.as_bytes());
    hasher.update(salt.as_bytes());
    hasher.update(secret.as_bytes());

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut hex, "{byte:02x}").expect("writing into a String cannot fail");
    }

    hex
}

/// A salt inside the range the provider asks for. It only has to differ between requests,
/// so the clock is enough and no random number generator is needed.
fn salt_from_clock() -> u64 {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_micros() as u64);

    SALT_MIN + micros % SALT_SPAN
}

/// Reads a variable that has to be present and non-empty.
fn required_env(name: &'static str) -> Result<String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => Err(Error::MissingEnv(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The expected hash was produced independently, outside this code:
    /// `printf '%s' '2015063000000001apple143566028812345678' | md5sum`
    #[test]
    fn signature_follows_the_documented_concatenation() {
        let sign = signature("2015063000000001", "apple", "1435660288", "12345678");
        assert_eq!(sign, "f89f9594663708c1605f3d736d01d2d4");
        assert_eq!(sign.len(), 32, "md5 in hex is 32 characters");
    }

    #[test]
    fn a_successful_response_yields_the_translation() {
        let body = r#"{"from":"zh","to":"en","trans_result":[{"src":"测试","dst":"test"}]}"#;
        assert_eq!(parse_response(body).unwrap(), "test");
    }

    #[test]
    fn one_response_line_per_input_line_is_joined() {
        let body = r#"{"trans_result":[{"src":"甲","dst":"first"},{"src":"乙","dst":"second"}]}"#;
        assert_eq!(parse_response(body).unwrap(), "first\nsecond");
    }

    #[test]
    fn a_refusal_becomes_a_provider_error() {
        let body = r#"{"error_code":"54001","error_msg":"Invalid Sign"}"#;
        let error = parse_response(body).unwrap_err();
        assert!(matches!(error, Error::Provider { code, .. } if code == "54001"));
    }

    #[test]
    fn a_body_without_a_translation_is_reported_as_unexpected() {
        assert!(matches!(
            parse_response("<html>not json</html>"),
            Err(Error::UnexpectedResponse(_))
        ));
        assert!(matches!(
            parse_response(r#"{"error_code":"52000","error_msg":"Success"}"#),
            Err(Error::UnexpectedResponse(_))
        ));
    }

    /// The endpoint written in the settings is the one that is used, without asking the
    /// environment: the file has the last word over whether the provider is reachable at all.
    #[test]
    fn settings_that_are_all_there_do_not_fall_back_to_the_environment() {
        let translator =
            BaiduTranslator::from_settings("an appid", "a secret", "https://example.invalid/")
                .expect("settings that hold everything must build");

        assert_eq!(translator.appid, "an appid");
        assert_eq!(translator.secret, "a secret");
        assert_eq!(translator.endpoint, "https://example.invalid/");
    }

    #[test]
    fn the_salt_lands_in_the_range_the_provider_wants() {
        let wanted = SALT_MIN..SALT_MIN + SALT_SPAN;
        for _ in 0..100 {
            assert!(wanted.contains(&salt_from_clock()));
        }
    }
}
