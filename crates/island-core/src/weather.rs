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
