//! Data for the start-page widgets (weather, quotes).
//!
//! The start page can't fetch anything itself (its CSP only allows its own origin), and it never
//! supplies an address: it names a request kind plus a few values, and only the fixed templates
//! below are ever fetched, with every value validated.
use serde_json::Value;
use std::time::Duration;
use url::form_urlencoded::byte_serialize;

const MAX_BODY_BYTES: u64 = 256 * 1024;

fn enc(s: &str) -> String {
    byte_serialize(s.as_bytes()).collect()
}

/// Address for a widget request, or None when the kind is unknown or a value is invalid.
pub fn request_url(kind: &str, params: &Value, lang: &str) -> Option<String> {
    match kind {
        "geocode" => {
            let q = params.get("q")?.as_str()?.trim();
            let ok = (2..=80).contains(&q.chars().count()) && !q.chars().any(char::is_control);
            ok.then(|| {
                format!(
                    "https://geocoding-api.open-meteo.com/v1/search?name={}&count=5&language={lang}&format=json",
                    enc(q)
                )
            })
        }
        "weather" => {
            let lat = params.get("lat")?.as_f64()?;
            let lon = params.get("lon")?.as_f64()?;
            let ok = lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0;
            ok.then(|| {
                format!(
                    "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
                     &current=temperature_2m,apparent_temperature,weather_code,wind_speed_10m,is_day\
                     &daily=weather_code,temperature_2m_max,temperature_2m_min\
                     &timezone=auto&forecast_days=4&wind_speed_unit=ms"
                )
            })
        }
        "quote" => {
            let symbol = params.get("symbol")?.as_str()?;
            let ok = (1..=20).contains(&symbol.len())
                && symbol
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".^=-".contains(&c));
            ok.then(|| {
                format!(
                    "https://query1.finance.yahoo.com/v8/finance/chart/{}?range=1d&interval=1d",
                    enc(symbol)
                )
            })
        }
        _ => None,
    }
}

/// Fetches a widget address (call off the UI thread). Returns the JSON text.
pub fn fetch(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    agent
        .get(url)
        .header(
            "User-Agent",
            &format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Viima/{}",
                crate::updater::CURRENT_VERSION
            ),
        )
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_string()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_url_templates() {
        let u = request_url("geocode", &json!({"q": " Oulu "}), "fi").unwrap();
        assert!(u.starts_with("https://geocoding-api.open-meteo.com/v1/search?name=Oulu&"));
        assert!(request_url("geocode", &json!({"q": "x"}), "fi").is_none());

        let w = request_url("weather", &json!({"lat": 65.01236, "lon": 25.46816}), "fi").unwrap();
        assert!(w.contains("latitude=65.0124&longitude=25.4682"));
        assert!(request_url("weather", &json!({"lat": 91.0, "lon": 0.0}), "fi").is_none());
        assert!(request_url("weather", &json!({"lat": "65"}), "fi").is_none());

        let q = request_url("quote", &json!({"symbol": "^OMXH25"}), "fi").unwrap();
        assert!(q.contains("/chart/%5EOMXH25?"));
        assert!(request_url("quote", &json!({"symbol": "A/../B"}), "fi").is_none());
        assert!(request_url("quote", &json!({"symbol": ""}), "fi").is_none());
        assert!(request_url("evil", &json!({}), "fi").is_none());
    }
}
