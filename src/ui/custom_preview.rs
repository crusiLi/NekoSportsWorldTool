//! Input revision and request id prevent stale network replies from becoming ready.
use crate::track::{
    custom::{CustomRing, CustomRouteSpec},
    free_custom::Snapshot,
};
use std::sync::Arc;
#[derive(Default)]
pub struct State {
    inputs: Option<(CustomRouteSpec, String, bool)>,
    revision: u64,
    pending: bool,
    pub snapshot: Option<Arc<Snapshot>>,
}
impl State {
    pub fn observe(&mut self, spec: CustomRouteSpec, key: String, free: bool) -> bool {
        let inputs = (spec, key, free);
        if self.inputs.as_ref() != Some(&inputs) {
            self.inputs = Some(inputs);
            self.revision = self.revision.wrapping_add(1);
            self.pending = false;
            self.snapshot = None;
            return true;
        }
        false
    }
    pub fn begin(&mut self) -> u64 {
        self.revision = self.revision.wrapping_add(1);
        self.pending = true;
        self.snapshot = None;
        self.revision
    }
    pub fn busy(&self) -> bool {
        self.pending
    }
    pub fn accept(&mut self, id: u64, ring: Option<CustomRing>) -> bool {
        if id != self.revision || !self.pending {
            return false;
        }
        self.pending = false;
        self.snapshot = ring.and_then(|ring| {
            self.inputs.as_ref().map(|(spec, _, _)| {
                Arc::new(Snapshot {
                    spec: spec.clone(),
                    ring,
                })
            })
        });
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_route_or_mode_discards_old_success_and_old_failure() {
        let mut state = State::default();
        let mut spec = CustomRouteSpec {
            origin: "a".into(),
            destination: "b".into(),
            ..Default::default()
        };
        state.observe(spec.clone(), "fixture-key".into(), true);
        let old = state.begin();
        spec.destination = "c".into();
        state.observe(spec.clone(), "fixture-key".into(), true);
        let current = state.begin();
        let ring = CustomRing {
            points_bd: vec![(39.9, 116.4), (39.901, 116.4)],
            leg_length_m: 100.,
            total_length_m: 200.,
            loop_mode: true,
        };
        assert!(!state.accept(old, Some(ring.clone())));
        assert!(state.busy());
        assert!(state.accept(current, Some(ring)));
        assert_eq!(state.snapshot.as_ref().unwrap().spec.destination, "c");
        state.observe(spec, "fixture-key".into(), false);
        assert!(state.snapshot.is_none());
        assert!(!state.accept(current, None));
    }
}
