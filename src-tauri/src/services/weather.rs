use std::sync::Mutex;
use std::time::{Duration, Instant};

use island_core::IpCityInfo;

use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(5);
// ip-api.com 免费版没有 HTTPS，换 ipwho.is（免费、HTTPS、免 key）
const IP_CITY_URL: &str = "https://ipwho.is/";
const IP_CITY_TTL: Duration = Duration::from_secs(30 * 60);

static IP_CITY_CACHE: Mutex<Option<(Instant, IpCityInfo)>> = Mutex::new(None);

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into()
}

fn fetch_json(url: &str) -> AppResult<serde_json::Value> {
    let mut resp = agent()
        .get(url)
        .call()
        .map_err(|e| AppError::new(format!("error.network: {e}")))?;
    resp.body_mut()
        .read_json()
        .map_err(|e| AppError::new(format!("error.network: 响应不是 JSON: {e}")))
}

pub fn ip_city() -> AppResult<IpCityInfo> {
    {
        let cache = IP_CITY_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, info)) = cache.as_ref() {
            if at.elapsed() < IP_CITY_TTL {
                return Ok(info.clone());
            }
        }
    }
    let data = fetch_json(IP_CITY_URL)?;
    let ok = data.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        return Err(AppError::new("error.network: IP 定位失败"));
    }
    let info = IpCityInfo {
        city: data.get("city").and_then(|v| v.as_str()).unwrap_or("").into(),
        region_name: data.get("region").and_then(|v| v.as_str()).unwrap_or("").into(),
        country: data.get("country").and_then(|v| v.as_str()).unwrap_or("").into(),
        lat: data.get("latitude").and_then(|v| v.as_f64()),
        lon: data.get("longitude").and_then(|v| v.as_f64()),
    };
    *IP_CITY_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), info.clone()));
    Ok(info)
}

pub fn geocode(city: &str, lang: &str) -> AppResult<serde_json::Value> {
    let lang = if lang.is_empty() { "zh" } else { lang };
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language={}",
        urlencoded(city),
        urlencoded(lang),
    );
    fetch_json(&url)
}

pub fn query(
    lat: f64,
    lon: f64,
    daily: Option<&str>,
    forecast_days: Option<u32>,
) -> AppResult<serde_json::Value> {
    let mut url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current_weather=true"
    );
    if let Some(daily) = daily {
        url.push_str(&format!("&daily={}", urlencoded(daily)));
    }
    if let Some(days) = forecast_days {
        url.push_str(&format!("&forecast_days={days}"));
    }
    fetch_json(&url)
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b',') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::urlencoded;

    #[test]
    fn urlencoded_keeps_query_friendly_chars() {
        assert_eq!(urlencoded("temperature_2m_max,temperature_2m_min"), "temperature_2m_max,temperature_2m_min");
    }

    #[test]
    fn urlencoded_escapes_non_ascii() {
        assert_eq!(urlencoded("北京"), "%E5%8C%97%E4%BA%AC");
    }
}
