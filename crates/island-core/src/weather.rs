use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpCityInfo {
    pub city: String,
    pub region_name: String,
    pub country: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

const MSN_BUNDLE_PREFIX: &str = "https://assets.msn.com/bundles/v1/weather/latest/common.";
const MSN_KEY_ANCHOR: &str = "weatherfalcon/\",path:\"";
const MSN_KEY_FIELD: &str = "apiKey:\"";

pub fn msn_bundle_url(page: &str) -> Option<&str> {
    let start = page.find(MSN_BUNDLE_PREFIX)?;
    let len = page[start..].find(".js")? + 3;
    Some(&page[start..start + len])
}

/// 取 weatherfalcon 配置块里的 apiKey
pub fn msn_api_key(bundle: &str) -> Option<&str> {
    let anchor = bundle.find(MSN_KEY_ANCHOR)?;
    let rest = &bundle[anchor..];
    let field = rest.find(MSN_KEY_FIELD)? + MSN_KEY_FIELD.len();
    let len = rest[field..].find('"')?;
    let key = &rest[field..field + len];
    (!key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_common_bundle_url() {
        let page = r#"<script src="https://assets.msn.com/bundles/v1/weather/latest/vendors.aa.js"></script><script src="https://assets.msn.com/bundles/v1/weather/latest/common.785e.js"></script>"#;
        assert_eq!(
            msn_bundle_url(page),
            Some("https://assets.msn.com/bundles/v1/weather/latest/common.785e.js")
        );
    }

    #[test]
    fn extracts_key_from_weatherfalcon_config() {
        let js = r#"x={apiKey:"other"};let C={endPoint:"https://api.msn.com/weatherfalcon/",path:"weather/activitysettings/watchlist",apiKey:"Abc123",appId:"9e21"}"#;
        assert_eq!(msn_api_key(js), Some("Abc123"));
    }

    #[test]
    fn rejects_non_alphanumeric_key() {
        let js = r#"weatherfalcon/",path:"p",apiKey:"a b""#;
        assert_eq!(msn_api_key(js), None);
    }
}
