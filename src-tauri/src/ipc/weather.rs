use island_core::IpCityInfo;

use crate::error::AppResult;
use crate::services;

use super::off_thread;

#[tauri::command]
pub async fn weather_ip_city() -> AppResult<IpCityInfo> {
    off_thread(services::weather::ip_city).await
}

#[tauri::command]
pub async fn weather_geocode(city: String, lang: String) -> AppResult<serde_json::Value> {
    off_thread(move || services::weather::geocode(&city, &lang)).await
}

#[tauri::command]
pub async fn weather_query(
    lat: f64,
    lon: f64,
    daily: Option<String>,
    forecast_days: Option<u32>,
) -> AppResult<serde_json::Value> {
    off_thread(move || services::weather::query(lat, lon, daily.as_deref(), forecast_days)).await
}

#[tauri::command]
pub async fn weather_msn_overview(
    lat: f64,
    lon: f64,
    locale: String,
) -> AppResult<serde_json::Value> {
    off_thread(move || services::weather::msn_overview(lat, lon, &locale)).await
}
