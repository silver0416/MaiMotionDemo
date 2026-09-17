//! V3.1 visible-demand window. Everything here is rank-only: it keeps promising
//! local roles alive in the beam and never enters a reported score.
use crate::scoring_v3::{ContactKey, RoleState, LOCAL_CLUSTER_RADIUS};
use crate::{Chart, Hand, Point};

pub const LOOKAHEAD_MAX_GROUPS: usize = 8;
pub const LOOKAHEAD_MAX_SECONDS: f64 = 0.8;
/// Below BASE_TRAVEL_WEIGHT (0.18/unit): each part is a distance the hands
/// still have to cover in some form, so the bias stays mild and optimistic.
pub const FUTURE_ROLE_WEIGHT: f64 = 0.15;
/// A later point this close to an earlier contact counts as the same target.
const SAME_TARGET_RADIUS: f64 = 0.12;
const EPS: f64 = 1e-8;

/// A new contact the player can already see: Tap, Hold, Touch, Touch Hold or
/// Slide head. Slide checkpoints and handovers are not demands.
#[derive(Clone, Copy, Debug)]
pub struct FutureDemand {
    pub key: ContactKey,
    pub time: f64,
    pub point: Point,
}

pub struct Lookahead {
    demands: Vec<FutureDemand>,
}

pub fn contact_key(note: &crate::Note) -> ContactKey {
    match note.touch_area.as_deref().and_then(|a| a.chars().next()) {
        Some(area) => ContactKey::Touch(area, note.button),
        None => ContactKey::Button(note.button),
    }
}

impl Lookahead {
    pub fn from_chart(chart: &Chart) -> Self {
        let mut demands: Vec<_> = chart
            .notes
            .iter()
            .filter(|n| n.has_head)
            .map(|n| FutureDemand {
                key: contact_key(n),
                time: n.time_seconds,
                point: n.position,
            })
            .collect();
        demands.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.key.cmp(&b.key)));
        Self { demands }
    }

    /// Demands strictly after `time`, up to 8 onset groups or 0.8 seconds.
    pub fn window(&self, time: f64) -> &[FutureDemand] {
        let start = self.demands.partition_point(|d| d.time <= time + EPS);
        let mut groups = 0;
        let mut last = f64::NEG_INFINITY;
        let mut end = start;
        while end < self.demands.len() {
            let d = &self.demands[end];
            if d.time > time + LOOKAHEAD_MAX_SECONDS + EPS {
                break;
            }
            if d.time > last + EPS {
                groups += 1;
                if groups > LOOKAHEAD_MAX_GROUPS {
                    break;
                }
                last = d.time;
            }
            end += 1;
        }
        &self.demands[start..end]
    }

    pub fn revisit(&self, key: ContactKey, time: f64) -> Option<f64> {
        self.window(time)
            .iter()
            .find(|d| d.key == key)
            .map(|d| d.time)
    }

    /// Several distinct buttons are demanded at exactly this instant.
    pub fn is_button_chord(&self, time: f64) -> bool {
        let from = self.demands.partition_point(|d| d.time < time - EPS);
        let mut buttons = self.demands[from..]
            .iter()
            .take_while(|d| d.time <= time + EPS)
            .filter(|d| matches!(d.key, ContactKey::Button(_)));
        buttons
            .next()
            .is_some_and(|first| buttons.any(|d| d.key != first.key))
    }

    pub fn point_recurs(&self, point: Point, time: f64) -> bool {
        self.window(time)
            .iter()
            .any(|d| d.point.distance(point) <= SAME_TARGET_RADIUS)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FutureParts {
    /// A soon-revisited key whose owner has moved away while the other hand is closer.
    pub ownership_readiness: f64,
    /// Both hands crowd one of two recurring anchors instead of covering both.
    pub anchor_readiness: f64,
    /// Both hands stay in a local cluster although the next demand leaves it.
    pub return_readiness: f64,
}
impl FutureParts {
    pub fn total(&self) -> f64 {
        FUTURE_ROLE_WEIGHT
            * (self.ownership_readiness + self.anchor_readiness + self.return_readiness)
    }
}

/// Distance still needed before two targets are covered by different hands.
/// Zero when the hands already sit on them; high when both crowd one side.
fn pair_distance(hands: [Point; 2], a: Point, b: Point) -> f64 {
    let [l, r] = hands;
    (l.distance(a) + r.distance(b)).min(r.distance(a) + l.distance(b))
}

/// `hands`: where each hand will be once it is free.
pub fn future_role(window: &[FutureDemand], hands: [Point; 2], roles: &RoleState) -> FutureParts {
    let mut parts = FutureParts::default();
    // Distinct keys with their occurrence count, in first-seen order.
    let mut keys: Vec<(ContactKey, usize, FutureDemand)> = vec![];
    for d in window {
        match keys.iter_mut().find(|(k, _, _)| *k == d.key) {
            Some(entry) => entry.1 += 1,
            None => keys.push((d.key, 1, *d)),
        }
    }
    for (key, _, first) in &keys {
        if let Some((owner, strength)) = roles.ownership.get(*key, first.time) {
            let other = if owner == Hand::L { Hand::R } else { Hand::L };
            let excess = hands[owner.index()].distance(first.point)
                - hands[other.index()].distance(first.point);
            parts.ownership_readiness += strength * excess.max(0.);
        }
    }
    let mut anchors: Vec<_> = keys.iter().filter(|(_, n, _)| *n >= 2).collect();
    anchors.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(a.2.time.total_cmp(&b.2.time))
            .then(a.0.cmp(&b.0))
    });
    if let [a, b, ..] = anchors[..] {
        parts.anchor_readiness = pair_distance(hands, a.2.point, b.2.point);
    }
    let mut center = Point::default();
    let mut size = 0;
    for d in window {
        // The first demand leaving the local cluster: one hand should be able
        // to exit while the other finishes the cluster.
        if size > 0 && d.point.distance(center) > LOCAL_CLUSTER_RADIUS {
            parts.return_readiness = pair_distance(hands, center, d.point);
            break;
        }
        size += 1;
        let w = 1. / size as f64;
        center = Point {
            x: center.x + (d.point.x - center.x) * w,
            y: center.y + (d.point.y - center.y) * w,
        };
    }
    parts
}
