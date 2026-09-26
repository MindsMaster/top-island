use serde::{Deserialize, Serialize};

use crate::settings::WeatherCity;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpCityInfo {
    pub city: String,
    pub region_name: String,
    pub country: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

const MSN_BUNDLE_PATH: &str = "/bundles/v1/weather/latest/common.";
const MSN_KEY_ANCHOR: &str = "weatherfalcon/\",path:\"";
const MSN_KEY_FIELD: &str = "apiKey:\"";

/// 国内外资源域名不同 只认路径
pub fn msn_bundle_url(page: &str) -> Option<&str> {
    let path = page.find(MSN_BUNDLE_PATH)?;
    let start = page[..path].rfind("https://")?;
    let end = path + page[path..].find(".js")? + 3;
    Some(&page[start..end])
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

/// 多语言名如 "东京都/東京都" "华盛顿州;華盛頓州" 取第一个
fn first_variant(s: &str) -> &str {
    s.split([';', '/']).next().unwrap_or(s).trim()
}

/// Nominatim jsonv2 结果 display_name 由小到大逗号分隔
pub fn parse_city_search(v: &serde_json::Value) -> Vec<WeatherCity> {
    let mut out: Vec<WeatherCity> = Vec::new();
    for hit in v.as_array().into_iter().flatten() {
        let str_of = |k: &str| hit.get(k).and_then(|x| x.as_str()).unwrap_or("");
        let coord = |k: &str| str_of(k).parse::<f64>().ok();
        let (Some(lat), Some(lon)) = (coord("lat"), coord("lon")) else {
            continue;
        };
        let parts: Vec<&str> = str_of("display_name")
            .split(',')
            .map(first_variant)
            .filter(|p| !p.is_empty() && !p.chars().all(|c| c.is_ascii_digit() || c == '-'))
            .collect();
        let name = match first_variant(str_of("name")) {
            "" => parts.first().copied().unwrap_or(""),
            n => n,
        };
        if name.is_empty() {
            continue;
        }
        let country = if parts.len() > 1 {
            parts[parts.len() - 1]
        } else {
            ""
        };
        let middle = if parts.len() > 2 {
            &parts[1..parts.len() - 1]
        } else {
            &[][..]
        };
        let admin = middle
            .iter()
            .rev()
            .take(2)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if out.iter().any(|c| c.name == name && c.admin == admin) {
            continue;
        }
        out.push(WeatherCity {
            id: format!(
                "{}{}",
                str_of("osm_type").chars().next().unwrap_or('x'),
                hit.get("osm_id").map(|x| x.to_string()).unwrap_or_default()
            ),
            name: name.into(),
            admin,
            country: country.into(),
            lat,
            lon,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(name: &str, display: &str, osm_id: u64) -> serde_json::Value {
        serde_json::json!({
            "osm_type": "relation", "osm_id": osm_id, "lat": "41.57", "lon": "120.43",
            "name": name, "display_name": display
        })
    }

    #[test]
    fn city_search_orders_admin_from_large_to_small() {
        let v = serde_json::json!([hit("朝阳县", "朝阳县, 朝阳市, 辽宁省, 中国", 2769824)]);
        let c = &parse_city_search(&v)[0];
        assert_eq!(c.name, "朝阳县");
        assert_eq!(c.admin, "辽宁省 朝阳市");
        assert_eq!(c.country, "中国");
        assert_eq!(c.id, "r2769824");
        assert_eq!(c.lat, 41.57);
    }

    #[test]
    fn city_search_takes_first_name_variant_and_drops_postcode() {
        let v = serde_json::json!([
            hit("东京都/東京都", "东京都/東京都, 日本", 1),
            hit(
                "西雅圖",
                "西雅圖, King County, 华盛顿州;華盛頓州, 98104, 美国;美國",
                2
            ),
        ]);
        let r = parse_city_search(&v);
        assert_eq!(
            (
                r[0].name.as_str(),
                r[0].admin.as_str(),
                r[0].country.as_str()
            ),
            ("东京都", "", "日本")
        );
        assert_eq!(
            (r[1].admin.as_str(), r[1].country.as_str()),
            ("华盛顿州 King County", "美国")
        );
    }

    #[test]
    fn city_search_merges_same_place_listed_twice() {
        let v = serde_json::json!([
            hit("朝阳县", "朝阳县, 朝阳市, 辽宁省, 中国", 1),
            hit("朝阳县", "朝阳县, 朝阳市, 辽宁省, 中国", 2),
            hit("朝阳区", "朝阳区, 北京市, 中国", 3),
        ]);
        let r = parse_city_search(&v);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].admin, "北京市");
    }

    #[test]
    fn city_search_skips_hits_without_coordinates() {
        let v = serde_json::json!([{ "name": "x", "display_name": "x, 中国" }]);
        assert!(parse_city_search(&v).is_empty());
    }

    #[test]
    fn finds_common_bundle_url() {
        let page = r#"<script src="https://assets.msn.com/bundles/v1/weather/latest/vendors.aa.js"></script><script src="https://assets.msn.com/bundles/v1/weather/latest/common.785e.js"></script>"#;
        assert_eq!(
            msn_bundle_url(page),
            Some("https://assets.msn.com/bundles/v1/weather/latest/common.785e.js")
        );
    }

    #[test]
    fn finds_bundle_on_cn_assets_host() {
        let page = r#"<script src="https://assets.msn.cn/bundles/v1/weather/latest/common.9ab.js"></script>"#;
        assert_eq!(
            msn_bundle_url(page),
            Some("https://assets.msn.cn/bundles/v1/weather/latest/common.9ab.js")
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
