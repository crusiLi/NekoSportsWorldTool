//! Free-run branch kept separate from 0.4's campus/custom/stadium flow.
use super::{
    client::ApiClient,
    flow::{RunOutcome, RunParams, VERIFY_DETAIL},
    submit::{submit_record, SubmitParams},
};
use crate::track::{
    free,
    generate_road::RouteMode,
    wire::{build_run_obs_object, obs_keys},
};
use serde_json::Value;

/// Shared offline builder for GUI submission and CLI dry-run. A GUI plan is
/// pinned before starting the worker, so a file reload cannot change its route.
pub fn build(
    params: &RunParams,
    anchor: crate::location::Coordinate,
) -> Result<crate::track::model::Track, String> {
    let mut track = match params.route_mode {
        RouteMode::Legacy => free::build_with_color(
            params.dist,
            params.dur,
            params.seed,
            params.start_ms,
            anchor,
            params.track_color_mode,
        )?,
        RouteMode::Road => {
            let road = if let Some(road) = &params.free_road_plan {
                road.clone()
            } else {
                let path = super::model::load_config().osm_path;
                if path.is_empty() {
                    return Err("请先导入 OSM 路网".into());
                }
                let bytes = std::fs::read(path).map_err(|e| format!("读取路网失败：{e}"))?;
                let net = route_planner::load_osm(&bytes)?;
                std::sync::Arc::new(free::plan_road(&net, anchor, params.dist, params.seed)?)
            };
            if !road.matches(anchor, params.dist, params.seed) {
                return Err("位置或方案已改变，请重新预览路线".into());
            }
            road.build(
                params.dist,
                params.dur,
                params.seed,
                params.start_ms,
                params.track_color_mode,
            )?
        }
        RouteMode::Custom => {
            return Err("自由跑目前支持自由环线和沿真实道路，请切换路线算法".into())
        }
    };
    if let Some(range) = params.manual_altitude_range {
        crate::track::altitude::override_bd_a_range(&mut track, range)?;
    } else if let Some(value) = params.manual_altitude {
        crate::track::altitude::override_bd_a(&mut track, value)?;
    }
    track.validate_consistency()?;
    Ok(track)
}

pub fn run(
    client: &mut ApiClient,
    params: &RunParams,
    log: &mut dyn FnMut(&str),
) -> Result<RunOutcome, String> {
    let sess = client.login.clone().ok_or("未登录")?;
    if client.identity.has_unconfigured_default_location() {
        return Err("请先设置城市和自由跑位置".into());
    }
    let anchor = client.identity.anchor_coordinate()?;
    // Build first. A routing failure must never silently submit a classic loop.
    let track = build(params, anchor)?;
    log("[自由跑] 不读取学校打卡点或校园围栏；拉取自由跑策略…");
    let pol = super::policy::fetch_policy_for_mode(client, params.run_mode)?;
    if track.totalTime > pol.valid_time && pol.valid_time > 0 {
        return Err(format!(
            "本次时长 {} 秒超过策略上限 {} 秒，请调整后重新预览",
            track.totalTime, pol.valid_time
        ));
    }
    let sp = SubmitParams {
        run_mode: params.run_mode,
        track,
        uid: sess.uid,
        selected_unid: sess.unid.parse().unwrap_or(0),
        policy: pol.policy,
        policy_ts: pol.timestamp,
        min_distance: pol.min_distance,
        weight: if sess.weight > 0. { sess.weight } else { 68. },
        face_check: params.face_check,
        five_point_json: "[]".into(),
        address: client.identity.city.clone(),
    };
    let result = submit_record(client, &sp, log)?;
    let object = build_run_obs_object(
        &sp.track,
        result.rrid,
        &result.uuid,
        sess.uid,
        &[],
        params.run_mode,
    )?;
    let keys = obs_keys(&sp.track, result.rrid, &result.uuid);
    let obs_ok = super::obs::upload_both_keys(client, &keys, &object.to_string().into_bytes(), log);
    std::thread::sleep(std::time::Duration::from_secs(2));
    let (mut complete, mut mode_matches) = (None, None);
    let detail_ok = match super::records::fetch_one_record(client, result.rrid) {
        Ok(detail) => {
            complete = detail.get("complete").and_then(Value::as_bool);
            mode_matches = detail
                .get("sportType")
                .and_then(Value::as_i64)
                .map(|t| t == params.run_mode.sport_type());
            if let Ok(mut slot) = VERIFY_DETAIL.lock() {
                *slot = Some(detail);
            }
            true
        }
        Err(e) => {
            log(&format!("详情未能获取：{e}"));
            false
        }
    };
    log(&format!("自由跑记录已提交；OBS {obs_ok}/2，详情获取={detail_ok}，达标={complete:?}，类型匹配={mode_matches:?}"));
    Ok(RunOutcome {
        result,
        obs_ok,
        detail_ok,
        complete,
        mode_matches,
    })
}
