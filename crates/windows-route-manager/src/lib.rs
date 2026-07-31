#![deny(unsafe_code)]

use std::{collections::HashSet, net::Ipv4Addr};

use ipnet::Ipv4Net;

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform;

#[cfg(windows)]
pub use platform::{
    IpHelperBackend, IpHelperError, create_address, delete_address, query_dad_state,
    snapshot_routes,
};

pub const MAX_SYSTEM_ROUTES: usize = 4_096;
pub const PROJECT_ROUTE_METRIC: u32 = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DadState {
    Invalid,
    Tentative,
    Duplicate,
    Deprecated,
    Preferred,
    Unknown(i32),
}

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

pub trait RouteBackend {
    type Error;

    /// Creates exactly one route.
    ///
    /// # Errors
    ///
    /// Returns the platform error without retrying or treating ambiguous completion as success.
    fn create(&mut self, route: RouteKey) -> Result<(), Self::Error>;

    /// Deletes exactly one route.
    ///
    /// # Errors
    ///
    /// Returns the platform error without broad prefix, interface, or table cleanup.
    fn delete(&mut self, route: RouteKey) -> Result<(), Self::Error>;
}

pub trait HostNetworkBackend: RouteBackend {
    /// Creates one exact non-persistent address.
    ///
    /// # Errors
    ///
    /// Returns the platform failure without treating a pre-existing address as project owned.
    fn create_address(
        &mut self,
        interface_luid: u64,
        address: Ipv4Addr,
        prefix_length: u8,
    ) -> Result<(), Self::Error>;

    /// Deletes one exact address.
    ///
    /// # Errors
    ///
    /// Returns the platform failure without interface-wide cleanup.
    fn delete_address(
        &mut self,
        interface_luid: u64,
        address: Ipv4Addr,
        prefix_length: u8,
    ) -> Result<(), Self::Error>;

    /// Queries the authoritative Duplicate Address Detection state.
    ///
    /// # Errors
    ///
    /// Returns the platform query failure without inferring availability.
    fn dad_state(
        &mut self,
        interface_luid: u64,
        address: Ipv4Addr,
        prefix_length: u8,
    ) -> Result<DadState, Self::Error>;

    fn wait_dad_poll(&mut self);
}

#[derive(Debug, Eq, PartialEq)]
pub enum ProvisionError<E> {
    AddressCreate(E),
    DadQuery {
        source: E,
        address_cleanup: Option<E>,
    },
    DadRejected {
        state: DadState,
        address_cleanup: Option<E>,
    },
    DadTimeout {
        address_cleanup: Option<E>,
    },
    Routes {
        source: ExecuteError<E>,
        address_cleanup: Option<E>,
    },
}

/// Creates an address, waits for authoritative DAD success, then applies the route transaction.
///
/// # Errors
///
/// Fails closed on invalid poll bounds, any non-tentative/non-preferred DAD state, timeout, query
/// failure, or route failure. Every post-create failure attempts exact address cleanup and returns
/// that cleanup failure alongside the original error.
pub fn provision_network<B: HostNetworkBackend>(
    backend: &mut B,
    interface_luid: u64,
    address: Ipv4Addr,
    prefix_length: u8,
    maximum_dad_polls: usize,
    routes: &ReconcilePlan,
) -> Result<(), ProvisionError<B::Error>> {
    backend
        .create_address(interface_luid, address, prefix_length)
        .map_err(ProvisionError::AddressCreate)?;
    if maximum_dad_polls == 0 {
        return Err(ProvisionError::DadTimeout {
            address_cleanup: backend
                .delete_address(interface_luid, address, prefix_length)
                .err(),
        });
    }
    for poll in 0..maximum_dad_polls {
        match backend.dad_state(interface_luid, address, prefix_length) {
            Ok(DadState::Preferred) => {
                return execute_plan(backend, routes).map_err(|source| ProvisionError::Routes {
                    source,
                    address_cleanup: backend
                        .delete_address(interface_luid, address, prefix_length)
                        .err(),
                });
            }
            Ok(DadState::Tentative) if poll + 1 < maximum_dad_polls => backend.wait_dad_poll(),
            Ok(DadState::Tentative) => break,
            Ok(state) => {
                return Err(ProvisionError::DadRejected {
                    state,
                    address_cleanup: backend
                        .delete_address(interface_luid, address, prefix_length)
                        .err(),
                });
            }
            Err(source) => {
                return Err(ProvisionError::DadQuery {
                    source,
                    address_cleanup: backend
                        .delete_address(interface_luid, address, prefix_length)
                        .err(),
                });
            }
        }
    }
    Err(ProvisionError::DadTimeout {
        address_cleanup: backend
            .delete_address(interface_luid, address, prefix_length)
            .err(),
    })
}

#[derive(Debug, Eq, PartialEq)]
pub enum ExecuteError<E> {
    Add {
        route: RouteKey,
        source: E,
        compensation_failures: Vec<(RouteKey, E)>,
    },
    Remove {
        route: RouteKey,
        source: E,
        restore_failures: Vec<(RouteKey, E)>,
    },
}

/// Executes one additions-first transaction with explicit compensation evidence.
///
/// # Errors
///
/// Returns the original create/delete failure together with every failed compensating operation.
/// The caller must treat any non-empty compensation or restore list as host state requiring manual
/// recovery; failures are never collapsed into a generic success result.
pub fn execute_plan<B: RouteBackend>(
    backend: &mut B,
    plan: &ReconcilePlan,
) -> Result<(), ExecuteError<B::Error>> {
    let mut added = Vec::with_capacity(plan.additions.len());
    for route in &plan.additions {
        if let Err(source) = backend.create(*route) {
            let mut compensation_failures = Vec::new();
            for added_route in added.iter().rev().copied() {
                if let Err(error) = backend.delete(added_route) {
                    compensation_failures.push((added_route, error));
                }
            }
            return Err(ExecuteError::Add {
                route: *route,
                source,
                compensation_failures,
            });
        }
        added.push(*route);
    }

    let mut removed = Vec::with_capacity(plan.removals.len());
    for route in &plan.removals {
        if let Err(source) = backend.delete(*route) {
            let mut restore_failures = Vec::new();
            for removed_route in removed.iter().rev().copied() {
                if let Err(error) = backend.create(removed_route) {
                    restore_failures.push((removed_route, error));
                }
            }
            return Err(ExecuteError::Remove {
                route: *route,
                source,
                restore_failures,
            });
        }
        removed.push(*route);
    }
    Ok(())
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
    use std::collections::VecDeque;

    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        calls: Vec<(&'static str, RouteKey)>,
        create_results: VecDeque<Result<(), &'static str>>,
        delete_results: VecDeque<Result<(), &'static str>>,
        address_create_results: VecDeque<Result<(), &'static str>>,
        address_delete_results: VecDeque<Result<(), &'static str>>,
        dad_results: VecDeque<Result<DadState, &'static str>>,
        host_calls: Vec<&'static str>,
    }

    impl RouteBackend for FakeBackend {
        type Error = &'static str;

        fn create(&mut self, route: RouteKey) -> Result<(), Self::Error> {
            self.calls.push(("create", route));
            self.create_results.pop_front().unwrap_or(Ok(()))
        }

        fn delete(&mut self, route: RouteKey) -> Result<(), Self::Error> {
            self.calls.push(("delete", route));
            self.delete_results.pop_front().unwrap_or(Ok(()))
        }
    }

    impl HostNetworkBackend for FakeBackend {
        fn create_address(
            &mut self,
            _interface_luid: u64,
            _address: Ipv4Addr,
            _prefix_length: u8,
        ) -> Result<(), Self::Error> {
            self.host_calls.push("address-create");
            self.address_create_results.pop_front().unwrap_or(Ok(()))
        }

        fn delete_address(
            &mut self,
            _interface_luid: u64,
            _address: Ipv4Addr,
            _prefix_length: u8,
        ) -> Result<(), Self::Error> {
            self.host_calls.push("address-delete");
            self.address_delete_results.pop_front().unwrap_or(Ok(()))
        }

        fn dad_state(
            &mut self,
            _interface_luid: u64,
            _address: Ipv4Addr,
            _prefix_length: u8,
        ) -> Result<DadState, Self::Error> {
            self.host_calls.push("dad");
            self.dad_results
                .pop_front()
                .unwrap_or(Ok(DadState::Tentative))
        }

        fn wait_dad_poll(&mut self) {
            self.host_calls.push("wait");
        }
    }

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

    #[test]
    fn add_failure_compensates_created_routes_in_reverse_order() {
        let first = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("first");
        let second = RouteKey::project(prefix("192.168.20.0/24"), 7).expect("second");
        let plan = ReconcilePlan {
            additions: vec![first, second],
            compensation: vec![second, first],
            removals: Vec::new(),
        };
        let mut backend = FakeBackend {
            create_results: [Ok(()), Err("create")].into(),
            ..FakeBackend::default()
        };
        assert_eq!(
            execute_plan(&mut backend, &plan),
            Err(ExecuteError::Add {
                route: second,
                source: "create",
                compensation_failures: Vec::new(),
            })
        );
        assert_eq!(
            backend.calls,
            vec![("create", first), ("create", second), ("delete", first)]
        );
    }

    #[test]
    fn compensation_failure_is_not_hidden() {
        let first = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("first");
        let second = RouteKey::project(prefix("192.168.20.0/24"), 7).expect("second");
        let plan = ReconcilePlan {
            additions: vec![first, second],
            compensation: vec![second, first],
            removals: Vec::new(),
        };
        let mut backend = FakeBackend {
            create_results: [Ok(()), Err("create")].into(),
            delete_results: [Err("rollback")].into(),
            ..FakeBackend::default()
        };
        assert_eq!(
            execute_plan(&mut backend, &plan),
            Err(ExecuteError::Add {
                route: second,
                source: "create",
                compensation_failures: vec![(first, "rollback")],
            })
        );
    }

    #[test]
    fn removal_failure_restores_only_routes_already_removed() {
        let first = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("first");
        let second = RouteKey::project(prefix("192.168.20.0/24"), 7).expect("second");
        let plan = ReconcilePlan {
            additions: Vec::new(),
            compensation: Vec::new(),
            removals: vec![first, second],
        };
        let mut backend = FakeBackend {
            create_results: [Err("restore")].into(),
            delete_results: [Ok(()), Err("delete")].into(),
            ..FakeBackend::default()
        };
        assert_eq!(
            execute_plan(&mut backend, &plan),
            Err(ExecuteError::Remove {
                route: second,
                source: "delete",
                restore_failures: vec![(first, "restore")],
            })
        );
        assert_eq!(
            backend.calls,
            vec![("delete", first), ("delete", second), ("create", first)]
        );
    }

    #[test]
    fn provision_waits_for_preferred_before_creating_routes() {
        let route = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("route");
        let plan = ReconcilePlan {
            additions: vec![route],
            compensation: vec![route],
            removals: Vec::new(),
        };
        let mut backend = FakeBackend {
            dad_results: [Ok(DadState::Tentative), Ok(DadState::Preferred)].into(),
            ..FakeBackend::default()
        };
        assert_eq!(
            provision_network(&mut backend, 7, Ipv4Addr::new(100, 88, 0, 10), 16, 3, &plan,),
            Ok(())
        );
        assert_eq!(
            backend.host_calls,
            vec!["address-create", "dad", "wait", "dad"]
        );
        assert_eq!(backend.calls, vec![("create", route)]);
    }

    #[test]
    fn duplicate_dad_deletes_address_without_touching_routes() {
        let mut backend = FakeBackend {
            dad_results: [Ok(DadState::Duplicate)].into(),
            ..FakeBackend::default()
        };
        assert_eq!(
            provision_network(
                &mut backend,
                7,
                Ipv4Addr::new(100, 88, 0, 10),
                16,
                3,
                &ReconcilePlan {
                    additions: Vec::new(),
                    compensation: Vec::new(),
                    removals: Vec::new(),
                },
            ),
            Err(ProvisionError::DadRejected {
                state: DadState::Duplicate,
                address_cleanup: None,
            })
        );
        assert_eq!(
            backend.host_calls,
            vec!["address-create", "dad", "address-delete"]
        );
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn route_failure_reports_address_cleanup_failure() {
        let route = RouteKey::project(prefix("192.168.10.0/24"), 7).expect("route");
        let plan = ReconcilePlan {
            additions: vec![route],
            compensation: vec![route],
            removals: Vec::new(),
        };
        let mut backend = FakeBackend {
            create_results: [Err("route")].into(),
            address_delete_results: [Err("address-cleanup")].into(),
            dad_results: [Ok(DadState::Preferred)].into(),
            ..FakeBackend::default()
        };
        assert_eq!(
            provision_network(&mut backend, 7, Ipv4Addr::new(100, 88, 0, 10), 16, 3, &plan,),
            Err(ProvisionError::Routes {
                source: ExecuteError::Add {
                    route,
                    source: "route",
                    compensation_failures: Vec::new(),
                },
                address_cleanup: Some("address-cleanup"),
            })
        );
    }
}
