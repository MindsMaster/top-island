use std::sync::Mutex;
use std::time::{Duration, Instant};

use island_core::{msn_api_key, msn_bundle_url, IpCityInfo};

use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(5);
const BUNDLE_TIMEOUT: Duration = Duration::from_secs(20);
/// ipwho.is 免费 HTTPS 免 key
const IP_CITY_URL: &str = "https://ipwho.is/";
const IP_CITY_TTL: Duration = Duration::from_secs(30 * 60);

const MSN_PAGE_URL: &str = "https://www.msn.com/zh-cn/weather/forecast";
const MSN_OVERVIEW_URL: &str = "https://api.msn.cn/weatherfalcon/weather/overview";
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

static MSN_KEY: Mutex<Option<String>> = Mutex::new(None);
static IP_CITY_CACHE: Mutex<Option<(Instant, IpCityInfo)>> = Mutex::new(None);

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

fn fetch_json(url: &str) -> AppResult<serde_json::Value> {
    let mut resp = agent(TIMEOUT)
        .get(url)
        .call()
        .map_err(|e| AppError::new(format!("error.network: {e}")))?;
    resp.body_mut()
        .read_json()
        .map_err(|e| AppError::new(format!("error.network: 响应不是 JSON: {e}")))
}

fn fetch_text(url: &str) -> AppResult<String> {
    let mut resp = agent(BUNDLE_TIMEOUT)
        .get(url)
        .header("User-Agent", BROWSER_UA)
        .call()
        .map_err(|e| AppError::new(format!("error.network: {e}")))?;
    resp.body_mut()
        .read_to_string()
        .map_err(|e| AppError::new(format!("error.network: {e}")))
}

/// 取自 MSN 天气网页前端包
fn acquire_msn_key() -> AppResult<String> {
    let page = fetch_text(MSN_PAGE_URL)?;
    let bundle_url = msn_bundle_url(&page)
        .ok_or_else(|| AppError::new("error.network: MSN 页面未找到前端包"))?;
    let bundle = fetch_text(bundle_url)?;
    msn_api_key(&bundle)
        .map(str::to_owned)
        .ok_or_else(|| AppError::new("error.network: MSN 前端包未找到 key"))
}

fn msn_key(refresh: bool) -> AppResult<String> {
    let mut cached = MSN_KEY.lock().unwrap_or_else(|e| e.into_inner());
    if let (false, Some(key)) = (refresh, cached.as_ref()) {
        return Ok(key.clone());
    }
    let key = acquire_msn_key()?;
    *cached = Some(key.clone());
    Ok(key)
}

pub fn msn_overview(lat: f64, lon: f64, locale: &str) -> AppResult<serde_json::Value> {
    let url = |key: &str| {
        format!(
            "{MSN_OVERVIEW_URL}?apikey={key}&ocid=msftweather&lat={lat}&lon={lon}&units=C&locale={}&days=7&wrapodata=false",
            urlencoded(locale),
        )
    };
    // key 会随发版轮换 失败重取一次
    match fetch_json(&url(&msn_key(false)?)) {
        Ok(data) => Ok(data),
        Err(_) => fetch_json(&url(&msn_key(true)?)),
    }
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
    let ok = data
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !ok {
        return Err(AppError::new("error.network: IP 定位失败"));
    }
    let info = IpCityInfo {
        city: data
            .get("city")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        region_name: data
            .get("region")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        country: data
            .get("country")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
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
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,weather_code,is_day&timezone=auto"
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
        assert_eq!(
            urlencoded("temperature_2m_max,temperature_2m_min"),
            "temperature_2m_max,temperature_2m_min"
        );
    }

    #[test]
    fn urlencoded_escapes_non_ascii() {
        assert_eq!(urlencoded("北京"), "%E5%8C%97%E4%BA%AC");
    }
}
