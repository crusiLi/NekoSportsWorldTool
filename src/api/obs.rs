//! OBS 上传：POST /api/obs/temporary/url 换签名 URL → PUT JSON。

use super::client::{get_field, parse_data_field, ApiClient};
use base64::Engine;
use serde_json::{json, Value};
use std::io::Read;

pub const OBS_SIGN_PATH: &str = "/api/obs/temporary/url";

/// 换取签名 URL。
pub fn sign_url(client: &mut ApiClient, method: &str, key: &str) -> Result<String, String> {
    let body = json!({
        "bucketName": "iydsj-hbase-hot",
        "objectKey": key,
        "method": method,
        "contentType": "application/json",
    })
    .to_string();
    let biz = client.call("POST", OBS_SIGN_PATH, &body, &[])?;
    // business.data 可能是 JSON 字符串 {"signedUrl": ...}
    let data = parse_data_field(&biz);
    let signed = data
        .get("signedUrl")
        .and_then(|v| v.as_str())
        .or_else(|| get_field(&biz, "signedUrl").and_then(|v| v.as_str()))
        .ok_or("OBS 签名响应缺 signedUrl")?;
    Ok(signed.to_string())
}

/// PUT 上传 OBS 对象。
pub fn put_object(
    signed_url: &str,
    payload: &[u8],
    log: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let agent = super::client::make_agent();
    let resp = agent
        .put(signed_url)
        .set("Content-Type", "application/json")
        .send_bytes(payload)
        .map_err(super::client::ureq_err)?;
    log(&format!(
        "[obs] PUT {} -> {}",
        shorten(signed_url),
        resp.status()
    ));
    Ok(())
}

/// 上传到两个 objectKey（详情页 + 兜底路径）。
pub fn upload_both_keys(
    client: &mut ApiClient,
    keys: &[String],
    payload: &[u8],
    log: &mut dyn FnMut(&str),
) -> usize {
    let mut ok = 0;
    for key in keys {
        match sign_url(client, "Put", key).and_then(|url| put_object(&url, payload, log)) {
            Ok(()) => ok += 1,
            Err(e) => log(&format!("[obs] PUT {key} 失败: {e}")),
        }
    }
    ok
}

fn shorten(url: &str) -> &str {
    // Never expose the signed query string.
    let start = url.find("run_data").unwrap_or(0);
    let end = url[start..]
        .find('?')
        .map(|n| start + n)
        .unwrap_or(url.len());
    &url[start..end]
}

/// 回读验证（GET 签名）。
pub fn fetch_object(
    client: &mut ApiClient,
    key: &str,
    log: &mut dyn FnMut(&str),
) -> Result<Value, String> {
    let signed = sign_url(client, "Get", key)?;
    let agent = super::client::make_agent();
    let resp = agent.get(&signed).call().map_err(super::client::ureq_err)?;
    let mut text = String::new();
    resp.into_reader()
        .take(32 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| "OBS 回读失败")?;
    if text.len() > 32 * 1024 * 1024 {
        return Err("OBS 对象过大".into());
    }
    log(&format!("[obs] GET 回读 {} 字节", text.len()));
    serde_json::from_str(&text).map_err(|e| format!("OBS 对象解析失败: {e}"))
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TrackReadback {
    pub checked: usize,
    pub matched: usize,
    pub total: usize,
}

fn track_points(object: &Value) -> Option<Value> {
    let encoded = object.get("run_data")?.as_str()?;
    if encoded.len() > 24 * 1024 * 1024 {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() > 32 * 1024 * 1024 {
        return None;
    }
    let value: Value = serde_json::from_slice(&data).ok()?;
    let points = if value.is_array() {
        value
    } else {
        serde_json::from_str(value.get("allLocJson")?.as_str()?).ok()?
    };
    if points.as_array()?.is_empty() {
        return None;
    }
    Some(points)
}

pub fn verify_uploaded_track(
    client: &mut ApiClient,
    keys: &[String],
    expected: &Value,
    log: &mut dyn FnMut(&str),
) -> TrackReadback {
    let mut result = TrackReadback {
        checked: 0,
        matched: 0,
        total: keys.len(),
    };
    let expected_points = track_points(expected);
    for key in keys {
        if let Ok(remote) = fetch_object(client, key, log) {
            result.checked += 1;
            if expected_points.is_some() && track_points(&remote) == expected_points {
                result.matched += 1;
            }
        }
    }
    log(&format!(
        "[obs] 轨迹回读：核对 {}/{}，一致 {}",
        result.checked, result.total, result.matched
    ));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn object(value: Value) -> Value {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(value.to_string().as_bytes()).unwrap();
        json!({"run_data":base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap())})
    }

    #[test]
    fn reads_direct_and_wrapped_points_but_rejects_empty_or_broken_data() {
        let points = json!([{ "lat":39.9,"lon":116.4,"time":1700000000000i64 }]);
        assert_eq!(track_points(&object(points.clone())), Some(points.clone()));
        assert_eq!(
            track_points(&object(json!({"allLocJson":points.to_string()}))),
            Some(points)
        );
        assert_eq!(track_points(&object(json!([]))), None);
        assert_eq!(track_points(&json!({"run_data":"invalid"})), None);
    }

    #[test]
    fn signed_query_is_never_in_log_shortening() {
        assert_eq!(
            shorten("https://example.com/run_data/foo?signature=secret"),
            "run_data/foo"
        );
    }
}
