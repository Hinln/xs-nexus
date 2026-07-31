#![deny(unsafe_code)]

use std::{collections::HashSet, net::Ipv4Addr};

use ipnet::Ipv4Net;

pub const MAX_SYSTEM_ROUTES: usize = 4_096;
pub const PROJECT_ROUTE_METRIC: u32 = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RouteKey {
    pub prefix: Ipv4Net,
    pub interface_luid: u64,
    pub next_hop: Ipv4Addr,
    pub metric: u32,
}

impl RouteKey {
    /// Builds one exact project-owned, on-link IPv4 route.
    ///
    /// # Errors
    ///
    /// Rejects default, host, reserved, non-canonical, zero-LUID, non-on-link, or variable-metric
    /// routes.
    pub fn project(prefix: Ipv4Net, interface_luid: u64) -> Result<Self, RoutePlanError> {
        if interface_luid == 0
            || !(1..=30).contains(&prefix.prefix_len())
            || prefix.network() != prefix.addr()
            || reserved(prefix)
        {
            return Err(RoutePlanError::UnsafeRoute);
        }
        Ok(Self {
            prefix,
            interface_luid,
            next_hop: Ipv4Addr::UNSPECIFIED,
            metric: PROJECT_ROUTE_METRIC,
        })
    }

    fn is_project_shape(self, expected_luid: u64) -> bool {
        self.interface_luid == expected_luid
            && self.next_hop == Ipv4Addr::UNSPECIFIED
            && self.metric == PROJECT_ROUTE_METRIC
            && Self::project(self.prefix, expected_luid) == Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemRoute {
    pub key: RouteKey,
    pub project_owned: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcilePlan {
    /// Routes created before any existing project route is removed.
    pub additions: Vec<RouteKey>,
    /// Exact additions to remove in reverse order if any create fails.
    pub compensation: Vec<RouteKey>,
    /// Exact manifest-owned routes removed only after all additions succeed.
    pub removals: Vec<RouteKey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutePlanError {
    RouteTableLimit,
    UnsafeRoute,
    DuplicateRoute,
    ForeignOverlap,
    OwnershipDrift,
}

/// Produces a fail-closed additions-first route transaction.
///
/// `owned` is the trusted local manifest. `system` is a bounded snapshot returned by Windows IP
/// Helper. The caller must free the native table after converting it into this owned snapshot.
///
/// # Errors
///
/// Rejects oversized snapshots, unsafe desired routes, duplicate entries, foreign overlap, and
/// any manifest entry whose exact shape or system ownership has drifted.
pub fn plan_reconcile(
    interface_luid: u64,
    desired: &[Ipv4Net],
    owned: &[RouteKey],
    system: &[SystemRoute],
) -> Result<ReconcilePlan, RoutePlanError> {
    if system.len() > MAX_SYSTEM_ROUTES {
        return Err(RoutePlanError::RouteTableLimit);
    }
    let desired = desired
        .iter()
        .copied()
        .map(|prefix| RouteKey::project(prefix, interface_luid))
        .collect::<Result<Vec<_>, _>>()?;
    require_unique(&desired)?;
    require_unique(owned)?;

    let system_keys = system.iter().map(|route| route.key).collect::<HashSet<_>>();
    for route in owned {
        if !route.is_project_shape(interface_luid)
            || !system_keys.contains(route)
            || system
                .iter()
                .find(|entry| entry.key == *route)
                .is_none_or(|entry| !entry.project_owned)
        {
            return Err(RoutePlanError::OwnershipDrift);
        }
    }

    for wanted in &desired {
        if system
            .iter()
            .any(|existing| !existing.project_owned && overlaps(existing.key.prefix, wanted.prefix))
        {
            return Err(RoutePlanError::ForeignOverlap);
        }
    }

    let owned_set = owned.iter().copied().collect::<HashSet<_>>();
    let desired_set = desired.iter().copied().collect::<HashSet<_>>();
    let mut additions = desired_set
        .difference(&owned_set)
        .copied()
        .collect::<Vec<_>>();
    let mut removals = owned_set
        .difference(&desired_set)
        .copied()
        .collect::<Vec<_>>();
    additions.sort_by_key(route_order);
    removals.sort_by_key(route_order);
    let compensation = additions.iter().rev().copied().collect();
    Ok(ReconcilePlan {
        additions,
        compensation,
        removals,
    })
}

fn require_unique(routes: &[RouteKey]) -> Result<(), RoutePlanError> {
    let mut unique = HashSet::with_capacity(routes.len());
    if routes.iter().copied().all(|route| unique.insert(route)) {
        Ok(())
    } else {
        Err(RoutePlanError::DuplicateRoute)
    }
}

fn route_order(route: &RouteKey) -> (u32, u8, u64, u32) {
    (
        u32::from(route.prefix.network()),
        route.prefix.prefix_len(),
        route.interface_luid,
        route.metric,
    )
}

fn overlaps(left: Ipv4Net, right: Ipv4Net) -> bool {
    left.contains(&right.network()) || right.contains(&left.network())
}

fn reserved(prefix: Ipv4Net) -> bool {
    [
        "0.0.0.0/8",
        "127.0.0.0/8",
        "169.254.0.0/16",
        "224.0.0.0/4",
        "240.0.0.0/4",
    ]
    .into_iter()
    .map(|value| value.parse::<Ipv4Net>().expect("constant IPv4 prefix"))
    .any(|blocked| overlaps(prefix, blocked))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(value: &str) -> Ipv4Net {
        value.parse().expect("valid prefix")
    }

    #[test]
    fn rejects_default_reserved_and_zero_luid() {
        assert_eq!(
            RouteKey::project(prefix("0.0.0.0/0"), 7),
            Err(RoutePlanError::UnsafeRoute)
        );
        assert_eq!(
            RouteKey::project(prefix("127.0.0.0/8"), 7),
            Err(RoutePlanError::UnsafeRoute)
        );
        assert_eq!(
            RouteKey::project(prefix("192.168.50.0/24"), 0),
            Err(RoutePlanError::UnsafeRoute)
        );
    }

    #[test]
    fn rejects_foreign_exact_or_partial_overlap() {
        let foreign = SystemRoute {
            key: RouteKey {
                prefix: prefix("192.168.0.0/16"),
                interface_luid: 9,
                next_hop: Ipv4Addr::new(192, 0, 2, 1),
                metric: 5,
            },
            project_owned: false,
        };
        assert_eq!(
            plan_reconcile(7, &[prefix("192.168.50.0/24")], &[], &[foreign]),
            Err(RoutePlanError::ForeignOverlap)
        );
    }

    #[test]
    fn plans_additions_first_reverse_compensation_and_exact_removal() {
        let old = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("old route");
        let system = [SystemRoute {
            key: old,
            project_owned: true,
        }];
        let plan = plan_reconcile(
            7,
            &[prefix("192.168.20.0/24"), prefix("192.168.30.0/24")],
            &[old],
            &system,
        )
        .expect("safe plan");
        assert_eq!(plan.additions.len(), 2);
        assert_eq!(
            plan.compensation,
            plan.additions.iter().rev().copied().collect::<Vec<_>>()
        );
        assert_eq!(plan.removals, vec![old]);
    }

    #[test]
    fn rejects_manifest_or_system_ownership_drift() {
        let owned = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("owned route");
        assert_eq!(
            plan_reconcile(7, &[], &[owned], &[]),
            Err(RoutePlanError::OwnershipDrift)
        );
        assert_eq!(
            plan_reconcile(
                7,
                &[],
                &[owned],
                &[SystemRoute {
                    key: owned,
                    project_owned: false,
                }],
            ),
            Err(RoutePlanError::OwnershipDrift)
        );
    }

    #[test]
    fn rejects_unbounded_system_snapshot() {
        let route = SystemRoute {
            key: RouteKey::project(prefix("192.168.10.0/24"), 7).expect("route"),
            project_owned: true,
        };
        let system = vec![route; MAX_SYSTEM_ROUTES + 1];
        assert_eq!(
            plan_reconcile(7, &[], &[], &system),
            Err(RoutePlanError::RouteTableLimit)
        );
    }
}
