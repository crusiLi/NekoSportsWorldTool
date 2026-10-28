//! 跑步提交：POST /api/v70260/runnings/save/record。
//! Android 身份 + runes/runef 头；body 31+ 字段 + signature/originalSign。

use super::client::{check_business, ureq_err, ApiClient};
use super::model::HOST;
use crate::crypto::decrypt::{decrypt_response, derive_paes_key};
use crate::crypto::envelope::{build_envelope, build_envelope_ts, rsa_public_key, OuterOrder};
use crate::crypto::header::{build_android_header, HeaderIdentity, UA_ANDROID};
use crate::crypto::sign::{original_sign, signature};
use crate::track::altitude;
use crate::track::calorie::{avg_power, official_kcal};
use crate::track::geom::round_to;
use crate::track::model::{GenPoint, Track};
use crate::track::wire::validate_five_point_wrapper;
use serde_json::{json, Map, Value};

pub const RECORD_PATH: &str = "/api/v70260/runnings/save/record";

/// Android 10s 窗（id 种子 60000）。
fn android_tensec(track: &Track, start_ms: i64, kind: &str, canonical: bool) -> Vec<Value> {
    let locs = &track.locations;
    let total_time = track.totalTime;
    let mut out = Vec::new();
    let rid_seed = 60000i64;
    let mut w = 10i64;
    while if canonical {
        w - 10 < total_time
    } else {
        w <= total_time
    } {
        let lo = w - 10;
        let hi = w.min(total_time);
        let mut d_lo = 0.0f64;
        let mut s_lo = 0i64;
        let mut d_hi = 0.0f64;
        let mut s_hi = 0i64;
        if canonical {
            (d_lo, s_lo) = crate::track::stats::counters_at(locs, lo);
            (d_hi, s_hi) = crate::track::stats::counters_at(locs, hi);
        } else {
            for p in locs {
                let tt = p.totalTime;
                if tt <= lo {
                    d_lo = p.totalDis;
                    s_lo = p.steps;
                }
                if tt <= hi {
                    d_hi = p.totalDis;
                    s_hi = p.steps;
                }
            }
        }
        let dist = round_to((d_hi - d_lo).max(0.0), 4);
        let steps_n = (s_hi - s_lo).max(0);
        let begin = start_ms + lo * 1000;
        let end = start_ms + hi * 1000;
        let qn = w / 10 - 1;
        if kind == "speed" {
            out.push(json!({
                "beginTime": begin, "distance": dist, "endTime": end,
                "flag": start_ms, "id": rid_seed + qn, "queueNum": qn, "state": 0,
            }));
        } else {
            out.push(json!({
                "avgDiff": 0.0, "beginTime": begin, "endTime": end,
                "flag": start_ms, "id": rid_seed + qn, "maxDiff": 0.0,
                "minDiff": 1000.0, "queueNum": qn, "state": 0, "stepsNum": steps_n,
            }));
        }
        w += 10;
    }
    out
}

/// bdA 正差分累计。
pub fn total_ascent(locs: &[GenPoint]) -> f64 {
    altitude::total_ascent(locs)
}

pub fn pace_speed_value(total_time_s: i64, distance_m: f64) -> i64 {
    let distance_m = (distance_m * 100.0).ceil() / 100.0;
    if distance_m <= 0.0 {
        return 0;
    }
    round_to(total_time_s as f64 / distance_m * 50.0 / 3.0 * 1000.0, 0) as i64
}

pub struct SubmitParams {
    pub run_mode: super::run_mode::RunMode,
    pub track: Track,
    pub uid: i64,
    pub selected_unid: i64,
    pub policy: i64,
    pub policy_ts: i64,
    pub min_distance: i64,
    pub weight: f64,
    pub face_check: i64,
    pub five_point_json: String,
    pub address: String,
}

#[allow(dead_code)]
pub struct SubmitResult {
    pub rrid: i64,
    pub uuid: String,
    pub start_ms: i64,
    pub complete: Option<bool>,
    pub total_dis: f64,
    pub total_time: i64,
    pub total_steps: i64,
    pub avg_step_freq: i64,
    pub calorie: i64,
    pub avg_power: i64,
    pub sel_distance: i64,
}

pub(crate) fn prepare_record(
    p: &SubmitParams,
    run_uuid: String,
) -> Result<(Value, SubmitResult), String> {
    let track = &p.track;
    let start_coordinate = track.validate_consistency()?;
    if p.five_point_json.is_empty() {
        return Err("五点轨迹不能为空".into());
    }
    if p.run_mode.is_free() {
        let points: Value = serde_json::from_str(&p.five_point_json).map_err(|e| e.to_string())?;
        if points != json!([]) {
            return Err("自由跑不应携带必经打卡点".into());
        }
    } else {
        validate_five_point_wrapper(&p.five_point_json)?;
    }
    let total_time = track.totalTime;
    let total_dis = track.totalDistance;
    let total_steps = track.totalSteps;
    let start_ms = track.startTime;
    let stop_ms = start_ms + total_time * 1000;
    let ascent = total_ascent(&track.locations);
    let power = avg_power(p.weight, total_dis, total_time);
    let kcal = official_kcal(p.weight, total_time, total_dis);

    let unid = p.selected_unid;

    let speed = pace_speed_value(total_time, total_dis);
    let avg_step_freq = 1i64.max(round_to(total_steps as f64 / total_time as f64 * 60.0, 0) as i64);

    let mut body = Map::new();
    let locations = if p.run_mode.is_free() {
        Value::Array(crate::track::wire::free_gps_points(track)).to_string()
    } else {
        String::new()
    };
    body.insert("allLocJson".into(), Value::String(locations));
    body.insert("sportType".into(), Value::from(p.run_mode.sport_type()));
    body.insert("policy".into(), Value::from(p.policy));
    body.insert("totalTime".into(), Value::from(total_time));
    body.insert("startTime".into(), Value::from(start_ms));
    body.insert("stopTime".into(), Value::from(stop_ms));
    body.insert("getPrize".into(), Value::Bool(false));
    body.insert("status".into(), Value::from(0));
    body.insert("uuid".into(), Value::String(run_uuid.clone()));
    body.insert("uid".into(), Value::from(p.uid));
    body.insert("selectedUnid".into(), Value::from(unid));
    body.insert("selRunTime".into(), Value::from(total_time));
    body.insert("selDistance".into(), Value::from(p.min_distance));
    body.insert(
        "totalDis".into(),
        Value::from(round_to(total_dis, 0) as i64),
    );
    body.insert("speed".into(), Value::from(speed));
    body.insert(
        "validDis".into(),
        Value::from(round_to(total_dis, 0) as i64),
    );
    body.insert("validTime".into(), Value::from(total_time));
    body.insert("complete".into(), Value::Bool(true));
    body.insert("unCompleteReason".into(), Value::from(0));
    body.insert("calorie".into(), Value::from(kcal));
    body.insert("totalSteps".into(), Value::from(total_steps));
    body.insert("avgStepFreq".into(), Value::from(avg_step_freq));
    body.insert("useMobilityTools".into(), Value::from(0));
    body.insert("faceCheck".into(), Value::from(p.face_check));
    body.insert(
        "totalAscent".into(),
        Value::from(round_to(ascent, 0) as i64),
    );
    body.insert("avgPower".into(), Value::from(power));
    body.insert(
        "speedPerTenSec".into(),
        Value::Array(android_tensec(
            track,
            start_ms,
            "speed",
            p.run_mode.is_free(),
        )),
    );
    body.insert(
        "stepsPerTenSec".into(),
        Value::Array(android_tensec(
            track,
            start_ms,
            "steps",
            p.run_mode.is_free(),
        )),
    );
    body.insert("isUpload".into(), Value::Bool(false));
    body.insert("more".into(), Value::Bool(false));
    body.insert("latitude".into(), Value::from(start_coordinate.latitude));
    body.insert("longitude".into(), Value::from(start_coordinate.longitude));
    body.insert("maxRunTime".into(), Value::from(0));
    body.insert("minSteps".into(), Value::from(0));
    if !p.five_point_json.is_empty() {
        body.insert(
            "fivePointJson".into(),
            Value::String(p.five_point_json.clone()),
        );
    }
    // Android 必现扩展字段
    body.insert("errorCode".into(), Value::from(0));
    body.insert("geeToken".into(), Value::String(String::new()));
    body.insert("unauthorized".into(), Value::from(0));
    body.insert("themeId".into(), Value::from(0));
    body.insert("goalId".into(), Value::Null);
    body.insert(
        "address".into(),
        Value::String(p.address.trim().to_string()),
    );

    let body_val = Value::Object(body.clone());
    let sig = signature(&body_val, false);
    let orig = original_sign(&body_val, false);
    body.insert("signature".into(), Value::String(sig));
    body.insert("originalSign".into(), Value::String(orig));
    let result = SubmitResult {
        rrid: 0,
        uuid: run_uuid,
        start_ms,
        complete: None,
        total_dis,
        total_time,
        total_steps,
        avg_step_freq,
        calorie: kcal,
        avg_power: power,
        sel_distance: p.min_distance,
    };
    Ok((Value::Object(body), result))
}

pub fn submit_record(
    client: &mut ApiClient,
    p: &SubmitParams,
    log: &mut dyn FnMut(&str),
) -> Result<SubmitResult, String> {
    let run_uuid = uuid::Uuid::new_v4().to_string().to_uppercase();
    let (body, mut result) = prepare_record(p, run_uuid.clone())?;
    let start_ms = result.start_ms;
    let body_plain = body.to_string();
    let device_id = if client.identity.device_id.is_empty() {
        uuid::Uuid::new_v4().to_string().to_uppercase()
    } else {
        client.identity.device_id.clone()
    };
    let android_identity = HeaderIdentity {
        platform: "android".into(),
        device_id: device_id.clone(),
        os_version: "14".into(),
        device_name: "22081212C".into(),
        ..client.identity.clone()
    };
    let (hp, hp_extra) =
        build_android_header(&android_identity, p.uid, &client_token(client), None);
    let header_env = build_envelope(&mut client.session, &hp, OuterOrder::Observed);
    let now = crate::crypto::envelope::now_ms();
    let body_env = build_envelope_ts(
        &mut client.session,
        &body_plain,
        OuterOrder::Insert,
        now + 1,
    );

    let runes = format!("{}{}", p.policy_ts, p.uid);
    let runef = format!("{}{}", run_uuid, start_ms);
    let mut req = client.agent.post(&format!("{HOST}{RECORD_PATH}"));
    req = req
        .set("Content-Type", "application/json; charset=utf-8")
        .set("User-Agent", UA_ANDROID)
        .set("appVersion", "7.3.40")
        .set("headerSign", &header_env.json)
        .set("runes", &runes)
        .set("runef", &runef);
    for (k, v) in &hp_extra {
        req = req.set(k, v);
    }
    let resp = req.send_string(&body_env.json).map_err(ureq_err)?;
    let status = resp.status();
    let raw = resp.into_string().unwrap_or_default();
    log(&format!(
        "[record] sportType={} HTTP {status} len={}",
        p.run_mode.sport_type(),
        raw.len()
    ));

    let key = derive_paes_key(
        &body_env.key_data[0],
        &body_env.key_data[1],
        &body_env.key_data[2],
        &body_env.key_data[3],
    );
    let dec = decrypt_response(raw.as_bytes(), &key, &rsa_public_key())
        .map_err(|e| format!("提交响应解密失败: {e}"))?;
    let biz = check_business(&dec.business)?;
    let rrid = super::client::get_field(&biz, "rrid")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if rrid <= 0 {
        return Err(format!("提交未返回 rrid: {}", truncate_json(&biz)));
    }
    log(&format!("√ 提交成功 rrid={rrid} uuid={run_uuid}"));
    result.rrid = rrid;
    result.complete = super::client::get_field(&biz, "complete").and_then(Value::as_bool);
    Ok(result)
}

#[cfg(test)]
mod metric_tests {
    use super::pace_speed_value;

    #[test]
    fn pace_uses_thousandths_of_minute_per_kilometre() {
        assert_eq!(pace_speed_value(2295, 5030.0), 7604);
    }
}

fn client_token(client: &ApiClient) -> String {
    client
        .login
        .as_ref()
        .map(|s| s.token.clone())
        .unwrap_or_default()
}

fn truncate_json(v: &Value) -> String {
    let s = v.to_string();
    s.chars().take(240).collect()
}

#[cfg(test)]
mod mode_tests {
    use super::*;
    use crate::api::run_mode::RunMode;

    fn params(mode: RunMode) -> SubmitParams {
        SubmitParams {
            run_mode: mode,
            track: crate::track::free::build_with_color(
                2000.0,
                800,
                42,
                1_700_000_000_000,
                crate::location::Coordinate::new(39.9, 116.4, 0.0).unwrap(),
                crate::api::model::TrackColorMode::FullGreen,
            )
            .unwrap(),
            uid: 123,
            selected_unid: 456,
            policy: 1,
            policy_ts: 1_700_000_000_000,
            min_distance: 1000,
            weight: 68.0,
            face_check: 0,
            five_point_json: "[]".into(),
            address: "北京市".into(),
        }
    }

    #[test]
    fn free_mode_is_in_both_signature_inputs_and_the_actual_body() {
        let p = params(RunMode::Free);
        let (body, _) = prepare_record(&p, "FIXED-TEST-UUID".into()).unwrap();
        assert_eq!(body["sportType"], 4);
        assert_eq!(body["fivePointJson"], "[]");
        assert!(body["originalSign"]
            .as_str()
            .unwrap()
            .contains("sportType=4&"));
        assert_eq!(body["signature"].as_str().unwrap(), signature(&body, false));
        let mut wrong_mode = body.clone();
        wrong_mode["sportType"] = json!(1);
        assert_ne!(
            signature(&wrong_mode, false),
            body["signature"].as_str().unwrap()
        );
    }

    #[test]
    fn campus_still_requires_real_points_and_free_rejects_them() {
        let mut p = params(RunMode::Campus);
        assert!(prepare_record(&p, "TEST".into()).is_err());
        let points = vec![json!({"lat":39.9,"lon":116.4,"glat":39.894,"glon":116.3935})];
        p.five_point_json = crate::track::wire::five_point_wrapper(&points, p.track.startTime);
        let (body, _) = prepare_record(&p, "TEST".into()).unwrap();
        assert_eq!(body["sportType"], 1);
        p.run_mode = RunMode::Free;
        assert!(prepare_record(&p, "TEST".into()).is_err());
    }

    #[test]
    fn free_obs_has_no_fabricated_checkpoints() {
        use base64::Engine;
        use std::io::Read;
        let p = params(RunMode::Free);
        let object = crate::track::wire::build_run_obs_object(
            &p.track,
            123,
            "TEST",
            456,
            &[],
            RunMode::Free,
        )
        .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(object["fixed_point_json"].as_str().unwrap())
            .unwrap();
        let mut decoded = String::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_string(&mut decoded)
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&decoded).unwrap(), json!([]));
        let (body, _) = prepare_record(&p, "TEST".into()).unwrap();
        let body_points: Value =
            serde_json::from_str(body["allLocJson"].as_str().unwrap()).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(object["run_data"].as_str().unwrap())
            .unwrap();
        let mut track_json = String::new();
        flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_string(&mut track_json)
            .unwrap();
        let obs_points: Value = serde_json::from_str(&track_json).unwrap();
        assert_eq!(body_points, obs_points);
        assert_eq!(
            obs_points.as_array().unwrap().len(),
            p.track.locations.len()
        );
        assert!(obs_points[0]["lat"].is_number() && obs_points[0]["lon"].is_number());
        assert!(obs_points[0]["time"].is_number() && obs_points[0]["steps"].is_number());
        assert!(crate::track::wire::build_run_obs_object(
            &p.track,
            123,
            "TEST",
            456,
            &[json!({})],
            RunMode::Free
        )
        .is_err());
    }

    #[test]
    fn free_body_and_obs_statistics_include_the_partial_tail() {
        let mut p = params(RunMode::Free);
        p.track = crate::track::free::build_with_color(
            2000.,
            803,
            42,
            1_700_000_000_000,
            crate::location::Coordinate::new(39.9, 116.4, 0.).unwrap(),
            crate::api::model::TrackColorMode::FullGreen,
        )
        .unwrap();
        // Final samples are the canonical source, including t=0.
        let mut first = p.track.locations[0].clone();
        first.totalTime = 0;
        first.totalDis = 0.;
        first.steps = 0;
        first.validTime = 0;
        first.validDis = 0.;
        p.track.locations.insert(0, first);
        crate::track::stats::rebuild(&mut p.track);
        let (body, _) = prepare_record(&p, "TAIL".into()).unwrap();
        let speed = body["speedPerTenSec"].as_array().unwrap();
        let steps = body["stepsPerTenSec"].as_array().unwrap();
        assert_eq!(speed.len(), 81);
        assert_eq!(steps.len(), 81);
        assert_eq!(
            speed.last().unwrap()["endTime"],
            p.track.startTime + 803_000
        );
        assert!(
            (speed
                .iter()
                .map(|w| w["distance"].as_f64().unwrap())
                .sum::<f64>()
                - p.track.totalDistance)
                .abs()
                < 0.01
        );
        assert_eq!(
            steps
                .iter()
                .map(|w| w["stepsNum"].as_i64().unwrap())
                .sum::<i64>(),
            p.track.totalSteps
        );
    }
}
