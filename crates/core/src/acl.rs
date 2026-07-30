use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
};

use serde::{Deserialize, Serialize};

use crate::ConfigurationPayload;

const MAX_RULES: usize = 4096;
const MAX_SELECTORS_PER_SIDE: usize = 32;
const MAX_PORT_RANGES: usize = 64;
const MAX_NAME_LENGTH: usize = 64;
const MAX_TAG_LENGTH: usize = 63;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AclAction {
    Allow,
    Deny,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AclProtocol {
    Any,
    Tcp,
    Udp,
    Icmp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AclSelector {
    Any,
    Node { node_id_base64: String },
    Group { name: String },
    Tag { name: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

impl PortRange {
    #[must_use]
    pub const fn contains(self, port: u16) -> bool {
        self.start <= port && port <= self.end
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AclRule {
    pub id: String,
    pub priority: u32,
    pub action: AclAction,
    pub sources: Vec<AclSelector>,
    pub destinations: Vec<AclSelector>,
    pub protocol: AclProtocol,
    #[serde(default)]
    pub destination_ports: Vec<PortRange>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AclDecisionReason {
    RuleMatch,
    DefaultDeny,
    UnknownSource,
    UnknownDestination,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AclDecision {
    pub allowed: bool,
    pub matched_rule_id: Option<String>,
    pub reason: AclDecisionReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AclValidationError {
    PolicyVersion,
    RuleCount,
    RuleOrder,
    DuplicateRule,
    InvalidRule,
    InvalidSelector,
    InvalidPortRange,
    DuplicateNode,
}

#[derive(Clone, Debug)]
struct AclSubject {
    node_id_base64: String,
    groups: HashSet<String>,
    tags: HashSet<String>,
}

#[derive(Clone, Debug)]
pub struct AclPolicy {
    rules: Vec<AclRule>,
    subjects: HashMap<Ipv4Addr, AclSubject>,
}

impl AclPolicy {
    /// Compiles and validates one signed configuration's ACL policy.
    ///
    /// # Errors
    ///
    /// Returns an [`AclValidationError`] when the policy is ambiguous or non-canonical.
    pub fn compile(configuration: &ConfigurationPayload) -> Result<Self, AclValidationError> {
        if configuration.policy_version == 0 {
            return Err(AclValidationError::PolicyVersion);
        }
        if configuration.policies.len() > MAX_RULES {
            return Err(AclValidationError::RuleCount);
        }

        let mut subjects = HashMap::with_capacity(configuration.nodes.len());
        for node in &configuration.nodes {
            let virtual_ip = node
                .virtual_ip
                .parse::<Ipv4Addr>()
                .map_err(|_| AclValidationError::DuplicateNode)?;
            if !canonical_group_set(&node.groups)
                || !canonical_tag_set(&node.tags)
                || subjects
                    .insert(
                        virtual_ip,
                        AclSubject {
                            node_id_base64: node.node_id_base64.clone(),
                            groups: node.groups.iter().cloned().collect(),
                            tags: node.tags.iter().cloned().collect(),
                        },
                    )
                    .is_some()
            {
                return Err(AclValidationError::DuplicateNode);
            }
        }

        let mut rule_ids = HashSet::with_capacity(configuration.policies.len());
        let mut previous: Option<&AclRule> = None;
        for rule in &configuration.policies {
            if let Some(previous) = previous
                && (previous.priority < rule.priority
                    || (previous.priority == rule.priority && previous.id >= rule.id))
            {
                return Err(AclValidationError::RuleOrder);
            }
            previous = Some(rule);
            if !rule_ids.insert(rule.id.as_str()) {
                return Err(AclValidationError::DuplicateRule);
            }
            validate_rule(rule)?;
        }

        Ok(Self {
            rules: configuration.policies.clone(),
            subjects,
        })
    }

    #[must_use]
    pub fn evaluate(
        &self,
        source: Ipv4Addr,
        destination: Ipv4Addr,
        protocol: AclProtocol,
        destination_port: Option<u16>,
    ) -> AclDecision {
        let Some(source_subject) = self.subjects.get(&source) else {
            return denied(AclDecisionReason::UnknownSource);
        };
        let Some(destination_subject) = self.subjects.get(&destination) else {
            return denied(AclDecisionReason::UnknownDestination);
        };

        for rule in &self.rules {
            if !selectors_match(&rule.sources, source_subject)
                || !selectors_match(&rule.destinations, destination_subject)
                || !protocol_matches(rule.protocol, protocol)
                || !port_matches(rule, destination_port)
            {
                continue;
            }
            return AclDecision {
                allowed: rule.action == AclAction::Allow,
                matched_rule_id: Some(rule.id.clone()),
                reason: AclDecisionReason::RuleMatch,
            };
        }
        denied(AclDecisionReason::DefaultDeny)
    }
}

fn validate_rule(rule: &AclRule) -> Result<(), AclValidationError> {
    if rule.priority == 0
        || !valid_acl_name(&rule.id)
        || rule.sources.is_empty()
        || rule.sources.len() > MAX_SELECTORS_PER_SIDE
        || rule.destinations.is_empty()
        || rule.destinations.len() > MAX_SELECTORS_PER_SIDE
        || rule.destination_ports.len() > MAX_PORT_RANGES
        || (!rule.destination_ports.is_empty()
            && !matches!(rule.protocol, AclProtocol::Tcp | AclProtocol::Udp))
    {
        return Err(AclValidationError::InvalidRule);
    }
    validate_selectors(&rule.sources)?;
    validate_selectors(&rule.destinations)?;

    let mut previous_end = None;
    for range in &rule.destination_ports {
        if range.start == 0
            || range.start > range.end
            || previous_end.is_some_and(|end| end >= range.start)
        {
            return Err(AclValidationError::InvalidPortRange);
        }
        previous_end = Some(range.end);
    }
    Ok(())
}

fn validate_selectors(selectors: &[AclSelector]) -> Result<(), AclValidationError> {
    let any_count = selectors
        .iter()
        .filter(|selector| matches!(selector, AclSelector::Any))
        .count();
    if any_count > 0 && selectors.len() != 1 {
        return Err(AclValidationError::InvalidSelector);
    }

    let mut canonical = HashSet::with_capacity(selectors.len());
    for selector in selectors {
        let key = match selector {
            AclSelector::Any => "any".to_owned(),
            AclSelector::Node { node_id_base64 } if valid_node_id_selector(node_id_base64) => {
                format!("node:{node_id_base64}")
            }
            AclSelector::Group { name } if valid_acl_name(name) => {
                format!("group:{name}")
            }
            AclSelector::Tag { name } if valid_tag(name) => {
                format!("tag:{name}")
            }
            _ => return Err(AclValidationError::InvalidSelector),
        };
        if !canonical.insert(key) {
            return Err(AclValidationError::InvalidSelector);
        }
    }
    Ok(())
}

fn selectors_match(selectors: &[AclSelector], subject: &AclSubject) -> bool {
    selectors.iter().any(|selector| match selector {
        AclSelector::Any => true,
        AclSelector::Node { node_id_base64 } => node_id_base64 == &subject.node_id_base64,
        AclSelector::Group { name } => subject.groups.contains(name),
        AclSelector::Tag { name } => subject.tags.contains(name),
    })
}

const fn protocol_matches(rule: AclProtocol, packet: AclProtocol) -> bool {
    matches!(rule, AclProtocol::Any)
        || matches!(
            (rule, packet),
            (AclProtocol::Tcp, AclProtocol::Tcp)
                | (AclProtocol::Udp, AclProtocol::Udp)
                | (AclProtocol::Icmp, AclProtocol::Icmp)
        )
}

fn port_matches(rule: &AclRule, destination_port: Option<u16>) -> bool {
    if rule.destination_ports.is_empty() {
        return true;
    }
    destination_port.is_some_and(|port| {
        rule.destination_ports
            .iter()
            .any(|range| range.contains(port))
    })
}

fn canonical_group_set(values: &[String]) -> bool {
    values.len() <= MAX_SELECTORS_PER_SIDE
        && values.iter().all(|value| valid_acl_name(value))
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn canonical_tag_set(values: &[String]) -> bool {
    values.len() <= MAX_SELECTORS_PER_SIDE
        && values.iter().all(|value| valid_tag(value))
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn valid_acl_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_NAME_LENGTH
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn valid_tag(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_TAG_LENGTH
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

fn valid_node_id_selector(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_NAME_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn denied(reason: AclDecisionReason) -> AclDecision {
    AclDecision {
        allowed: false,
        matched_rule_id: None,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use crate::{ConfigurationNode, ConfigurationPayload};

    use super::*;

    fn node(id: &str, address: &str, groups: &[&str], tags: &[&str]) -> ConfigurationNode {
        ConfigurationNode {
            node_id_base64: id.to_owned(),
            identity_public_key_base64: format!("{id}-key"),
            virtual_ip: address.to_owned(),
            direct_endpoints: Vec::new(),
            candidates: Vec::new(),
            credential_serial: 1,
            credential_not_after: Utc::now(),
            role_bitmap: 0,
            groups: groups.iter().map(|value| (*value).to_owned()).collect(),
            tags: tags.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    fn policy(rules: Vec<AclRule>) -> ConfigurationPayload {
        ConfigurationPayload {
            schema_version: 1,
            network_id: Uuid::nil(),
            version: 1,
            policy_version: 1,
            generated_at: Utc::now(),
            address_pool: "100.88.0.0/24".to_owned(),
            discovery_endpoints: Vec::new(),
            nodes: vec![
                node("node-a", "100.88.0.16", &["clients"], &["linux"]),
                node("node-b", "100.88.0.17", &["servers"], &["database"]),
            ],
            relays: Vec::new(),
            policies: rules,
        }
    }

    fn rule(
        id: &str,
        priority: u32,
        action: AclAction,
        sources: Vec<AclSelector>,
        destinations: Vec<AclSelector>,
        protocol: AclProtocol,
        destination_ports: Vec<PortRange>,
    ) -> AclRule {
        AclRule {
            id: id.to_owned(),
            priority,
            action,
            sources,
            destinations,
            protocol,
            destination_ports,
        }
    }

    #[test]
    fn empty_policy_is_default_deny() {
        let policy = AclPolicy::compile(&policy(Vec::new())).expect("valid empty policy");
        let decision = policy.evaluate(
            "100.88.0.16".parse().expect("source"),
            "100.88.0.17".parse().expect("destination"),
            AclProtocol::Icmp,
            None,
        );
        assert_eq!(decision.reason, AclDecisionReason::DefaultDeny);
        assert!(!decision.allowed);
    }

    #[test]
    fn priority_and_ports_choose_first_matching_rule() {
        let policy = AclPolicy::compile(&policy(vec![
            rule(
                "deny-database-admin",
                200,
                AclAction::Deny,
                vec![AclSelector::Group {
                    name: "clients".to_owned(),
                }],
                vec![AclSelector::Tag {
                    name: "database".to_owned(),
                }],
                AclProtocol::Tcp,
                vec![PortRange {
                    start: 5432,
                    end: 5432,
                }],
            ),
            rule(
                "allow-client-services",
                100,
                AclAction::Allow,
                vec![AclSelector::Tag {
                    name: "linux".to_owned(),
                }],
                vec![AclSelector::Node {
                    node_id_base64: "node-b".to_owned(),
                }],
                AclProtocol::Tcp,
                vec![PortRange {
                    start: 1,
                    end: 65535,
                }],
            ),
        ]))
        .expect("valid policy");

        let denied = policy.evaluate(
            "100.88.0.16".parse().expect("source"),
            "100.88.0.17".parse().expect("destination"),
            AclProtocol::Tcp,
            Some(5432),
        );
        assert_eq!(
            denied.matched_rule_id.as_deref(),
            Some("deny-database-admin")
        );
        assert!(!denied.allowed);

        let allowed = policy.evaluate(
            "100.88.0.16".parse().expect("source"),
            "100.88.0.17".parse().expect("destination"),
            AclProtocol::Tcp,
            Some(443),
        );
        assert_eq!(
            allowed.matched_rule_id.as_deref(),
            Some("allow-client-services")
        );
        assert!(allowed.allowed);
    }

    #[test]
    fn policy_rejects_non_canonical_or_ambiguous_selectors() {
        let mut unsorted = policy(vec![
            rule(
                "low",
                1,
                AclAction::Allow,
                vec![AclSelector::Any],
                vec![AclSelector::Any],
                AclProtocol::Icmp,
                Vec::new(),
            ),
            rule(
                "high",
                2,
                AclAction::Allow,
                vec![AclSelector::Any],
                vec![AclSelector::Any],
                AclProtocol::Icmp,
                Vec::new(),
            ),
        ]);
        assert_eq!(
            AclPolicy::compile(&unsorted).expect_err("order must fail"),
            AclValidationError::RuleOrder
        );

        unsorted.policies = vec![rule(
            "ambiguous-selector",
            1,
            AclAction::Allow,
            vec![
                AclSelector::Any,
                AclSelector::Group {
                    name: "missing".to_owned(),
                },
            ],
            vec![AclSelector::Any],
            AclProtocol::Icmp,
            Vec::new(),
        )];
        assert_eq!(
            AclPolicy::compile(&unsorted).expect_err("selector must fail"),
            AclValidationError::InvalidSelector
        );
    }

    #[test]
    fn policy_accepts_existing_role_tag_syntax() {
        let tag = "service:database/primary";
        let mut configuration = policy(vec![rule(
            "allow-service-tag",
            1,
            AclAction::Allow,
            vec![AclSelector::Tag {
                name: tag.to_owned(),
            }],
            vec![AclSelector::Any],
            AclProtocol::Icmp,
            Vec::new(),
        )]);
        configuration.nodes[0].tags = vec![tag.to_owned()];

        let policy = AclPolicy::compile(&configuration).expect("role tag syntax remains valid");
        let decision = policy.evaluate(
            "100.88.0.16".parse().expect("source"),
            "100.88.0.17".parse().expect("destination"),
            AclProtocol::Icmp,
            None,
        );
        assert!(decision.allowed);
    }

    #[test]
    fn policy_rejects_invalid_role_tag_syntax() {
        let configuration = policy(vec![rule(
            "invalid-tag",
            1,
            AclAction::Allow,
            vec![AclSelector::Tag {
                name: "Service:Database".to_owned(),
            }],
            vec![AclSelector::Any],
            AclProtocol::Icmp,
            Vec::new(),
        )]);

        assert_eq!(
            AclPolicy::compile(&configuration).expect_err("uppercase tag must fail"),
            AclValidationError::InvalidSelector
        );
    }
}
