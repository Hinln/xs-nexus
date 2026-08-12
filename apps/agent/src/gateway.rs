use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
    path::PathBuf,
};

use futures_util::TryStreamExt as _;
use ipnet::Ipv4Net;
use netlink_sys::{Socket, SocketAddr, protocols::NETLINK_NETFILTER};
use rtnetlink::{
    Handle, RouteMessageBuilder,
    packet_route::{
        link::LinkAttribute,
        route::{RouteAddress, RouteAttribute, RouteProtocol, RouteScope},
    },
};
use serde::{Deserialize, Serialize};
use xs_core::{ResolvedSubnetRoute, SubnetRouteMode};

use crate::error::{AgentError, Result};

const NFT_TABLE_PREFIX: &str = "xs_nexus_";
const NFT_CHAIN_NAME: &str = "postrouting";
const NFT_OWNER_MARKER: &[u8] = b"xs-nexus-agent-v1";
const NETLINK_HEADER_LENGTH: usize = 16;
const NFGENMSG_LENGTH: usize = 4;
const NLMSG_ERROR: u16 = 2;
const NFNL_MSG_BATCH_BEGIN: u16 = 0x10;
const NFNL_MSG_BATCH_END: u16 = 0x11;
const NLM_F_REQUEST: u16 = 0x01;
const NLM_F_ACK: u16 = 0x04;
const NLM_F_EXCL: u16 = 0x200;
const NLM_F_CREATE: u16 = 0x400;
const NLM_F_APPEND: u16 = 0x800;
const NLA_F_NESTED: u16 = 0x8000;
const NFNL_SUBSYS_NFTABLES: u16 = 10;
const NFPROTO_IPV4: u8 = 2;
const NFT_MSG_NEWTABLE: u8 = 0;
const NFT_MSG_DELTABLE: u8 = 2;
const NFT_MSG_NEWCHAIN: u8 = 3;
const NFT_MSG_NEWRULE: u8 = 6;
const NFTA_TABLE_NAME: u16 = 1;
const NFTA_TABLE_USERDATA: u16 = 6;
const NFTA_CHAIN_TABLE: u16 = 1;
const NFTA_CHAIN_NAME: u16 = 3;
const NFTA_CHAIN_HOOK: u16 = 4;
const NFTA_CHAIN_TYPE: u16 = 7;
const NFTA_CHAIN_USERDATA: u16 = 12;
const NFTA_HOOK_HOOKNUM: u16 = 1;
const NFTA_HOOK_PRIORITY: u16 = 2;
const NF_INET_POST_ROUTING: u32 = 4;
const NF_IP_PRI_NAT_SRC: i32 = 100;
const NFTA_RULE_TABLE: u16 = 1;
const NFTA_RULE_CHAIN: u16 = 2;
const NFTA_RULE_EXPRESSIONS: u16 = 4;
const NFTA_RULE_USERDATA: u16 = 7;
const NFTA_LIST_ELEM: u16 = 1;
const NFTA_EXPR_NAME: u16 = 1;
const NFTA_EXPR_DATA: u16 = 2;
const NFTA_META_DREG: u16 = 1;
const NFTA_META_KEY: u16 = 2;
const NFT_META_IIFNAME: u32 = 6;
const NFT_META_OIFNAME: u32 = 7;
const NFTA_CMP_SREG: u16 = 1;
const NFTA_CMP_OP: u16 = 2;
const NFTA_CMP_DATA: u16 = 3;
const NFT_CMP_EQ: u32 = 0;
const NFTA_DATA_VALUE: u16 = 1;
const NFTA_PAYLOAD_DREG: u16 = 1;
const NFTA_PAYLOAD_BASE: u16 = 2;
const NFTA_PAYLOAD_OFFSET: u16 = 3;
const NFTA_PAYLOAD_LEN: u16 = 4;
const NFT_PAYLOAD_NETWORK_HEADER: u32 = 1;
const IPV4_DESTINATION_OFFSET: u32 = 16;
const NFTA_BITWISE_SREG: u16 = 1;
const NFTA_BITWISE_DREG: u16 = 2;
const NFTA_BITWISE_LEN: u16 = 3;
const NFTA_BITWISE_MASK: u16 = 4;
const NFTA_BITWISE_XOR: u16 = 5;
const NFT_REGISTER_1: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GatewayRoute {
    pub(crate) prefix: Ipv4Net,
    pub(crate) interface_name: String,
    pub(crate) mode: SubnetRouteMode,
}

impl GatewayRoute {
    pub(crate) fn from_resolved(route: ResolvedSubnetRoute<'_>) -> Self {
        Self {
            prefix: route.prefix,
            interface_name: route.interface_name.to_owned(),
            mode: route.mode,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForwardingRecord {
    pub(crate) interface_name: String,
    pub(crate) previous_value: u8,
}

pub(crate) fn table_name(project_interface_name: &str) -> String {
    format!("{NFT_TABLE_PREFIX}{project_interface_name}")
}

pub(crate) async fn validate_gateway_routes(
    handle: &Handle,
    routes: &[GatewayRoute],
    project_interface_name: &str,
) -> Result<()> {
    if routes.is_empty() {
        return Ok(());
    }
    let mut interface_names = HashMap::new();
    let mut links = handle.link().get().execute();
    while let Some(link) = links.try_next().await.map_err(|_| AgentError::Network)? {
        if let Some(name) = link
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                LinkAttribute::IfName(name) => Some(name.clone()),
                _ => None,
            })
        {
            interface_names.insert(link.header.index, name);
        }
    }
    if routes.iter().any(|route| {
        route.interface_name == project_interface_name
            || !interface_names
                .values()
                .any(|name| name == &route.interface_name)
    }) {
        return Err(AgentError::Network);
    }

    let desired = routes
        .iter()
        .map(|route| (route.prefix, route.interface_name.as_str()))
        .collect::<HashSet<_>>();
    let mut found = HashSet::new();
    let mut kernel_routes = handle
        .route()
        .get(RouteMessageBuilder::<Ipv4Addr>::new().build())
        .execute();
    while let Some(route) = kernel_routes
        .try_next()
        .await
        .map_err(|_| AgentError::Network)?
    {
        if route.header.destination_prefix_length == 0
            || route.header.protocol != RouteProtocol::Kernel
            || route.header.scope != RouteScope::Link
            || route_table(&route) != 254
        {
            continue;
        }
        let destination = route
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                RouteAttribute::Destination(RouteAddress::Inet(address)) => Some(*address),
                _ => None,
            });
        let output_interface = route
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                RouteAttribute::Oif(index) => Some(*index),
                _ => None,
            });
        let (Some(destination), Some(output_interface)) = (destination, output_interface) else {
            continue;
        };
        let Some(interface_name) = interface_names.get(&output_interface) else {
            continue;
        };
        let prefix = Ipv4Net::new(destination, route.header.destination_prefix_length)
            .map_err(|_| AgentError::Network)?;
        if desired.contains(&(prefix, interface_name.as_str())) {
            found.insert((prefix, interface_name.as_str()));
        }
    }
    if found.len() != desired.len() {
        return Err(AgentError::Network);
    }
    Ok(())
}

pub(crate) fn forwarding_value(interface_name: &str) -> Result<u8> {
    let value = std::fs::read_to_string(forwarding_path(interface_name))
        .map_err(|_| AgentError::Network)?;
    match value.trim() {
        "0" => Ok(0),
        "1" => Ok(1),
        _ => Err(AgentError::Network),
    }
}

pub(crate) fn set_forwarding(interface_name: &str, value: u8) -> Result<()> {
    if value > 1 {
        return Err(AgentError::Network);
    }
    if forwarding_value(interface_name)? == value {
        return Ok(());
    }
    std::fs::write(forwarding_path(interface_name), value.to_string())
        .map_err(|_| AgentError::Network)
}

pub(crate) fn forwarding_path_exists(interface_name: &str) -> bool {
    forwarding_path(interface_name).is_file()
}

pub(crate) fn restore_forwarding(records: &[ForwardingRecord]) -> Result<()> {
    let mut restored = true;
    for record in records {
        if set_forwarding(&record.interface_name, record.previous_value).is_err() {
            restored = false;
        }
    }
    if !restored {
        return Err(AgentError::Network);
    }
    Ok(())
}

pub(crate) fn replace_nat_table(
    table_name: &str,
    project_interface_name: &str,
    previously_owned: bool,
    routes: &[GatewayRoute],
) -> Result<()> {
    let nat_routes = routes
        .iter()
        .filter(|route| route.mode == SubnetRouteMode::Nat)
        .collect::<Vec<_>>();
    if previously_owned {
        delete_nat_table(table_name)?;
    }
    if nat_routes.is_empty() {
        return Ok(());
    }
    let mut client = NftClient::connect()?;
    client.create_table(table_name)?;
    if let Err(error) = client.create_chain(table_name).and_then(|()| {
        for route in nat_routes {
            client.create_nat_rule(table_name, project_interface_name, route)?;
        }
        Ok(())
    }) {
        let _ = client.delete_table(table_name, true);
        return Err(error);
    }
    Ok(())
}

pub(crate) fn delete_nat_table(table_name: &str) -> Result<()> {
    NftClient::connect()?.delete_table(table_name, true)
}

fn forwarding_path(interface_name: &str) -> PathBuf {
    PathBuf::from("/proc/sys/net/ipv4/conf")
        .join(interface_name)
        .join("forwarding")
}

fn route_table(route: &rtnetlink::packet_route::route::RouteMessage) -> u32 {
    route
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            RouteAttribute::Table(table) => Some(*table),
            _ => None,
        })
        .unwrap_or_else(|| u32::from(route.header.table))
}

struct NftClient {
    socket: Socket,
    sequence: u32,
}

impl NftClient {
    fn connect() -> Result<Self> {
        let mut socket = Socket::new(NETLINK_NETFILTER).map_err(|_| AgentError::Network)?;
        socket.bind_auto().map_err(|_| AgentError::Network)?;
        socket
            .connect(&SocketAddr::new(0, 0))
            .map_err(|_| AgentError::Network)?;
        Ok(Self {
            socket,
            sequence: 1,
        })
    }

    fn create_table(&mut self, table_name: &str) -> Result<()> {
        let mut attributes = Vec::new();
        push_string_attribute(&mut attributes, NFTA_TABLE_NAME, table_name);
        push_attribute(&mut attributes, NFTA_TABLE_USERDATA, NFT_OWNER_MARKER);
        self.request(
            NFT_MSG_NEWTABLE,
            NLM_F_CREATE | NLM_F_EXCL,
            &attributes,
            false,
        )
    }

    fn create_chain(&mut self, table_name: &str) -> Result<()> {
        let mut hook = Vec::new();
        push_u32_attribute(&mut hook, NFTA_HOOK_HOOKNUM, NF_INET_POST_ROUTING);
        push_i32_attribute(&mut hook, NFTA_HOOK_PRIORITY, NF_IP_PRI_NAT_SRC);
        let mut attributes = Vec::new();
        push_string_attribute(&mut attributes, NFTA_CHAIN_TABLE, table_name);
        push_string_attribute(&mut attributes, NFTA_CHAIN_NAME, NFT_CHAIN_NAME);
        push_nested_attribute(&mut attributes, NFTA_CHAIN_HOOK, &hook);
        push_string_attribute(&mut attributes, NFTA_CHAIN_TYPE, "nat");
        push_attribute(&mut attributes, NFTA_CHAIN_USERDATA, NFT_OWNER_MARKER);
        self.request(
            NFT_MSG_NEWCHAIN,
            NLM_F_CREATE | NLM_F_EXCL,
            &attributes,
            false,
        )
    }

    fn create_nat_rule(
        &mut self,
        table_name: &str,
        project_interface_name: &str,
        route: &GatewayRoute,
    ) -> Result<()> {
        let mut expressions = Vec::new();
        push_expression(&mut expressions, "meta", &meta_expression(NFT_META_IIFNAME));
        push_expression(
            &mut expressions,
            "cmp",
            &comparison_expression(&interface_name_bytes(project_interface_name)),
        );
        push_expression(&mut expressions, "meta", &meta_expression(NFT_META_OIFNAME));
        push_expression(
            &mut expressions,
            "cmp",
            &comparison_expression(&interface_name_bytes(&route.interface_name)),
        );
        push_expression(&mut expressions, "payload", &ipv4_destination_expression());
        if route.prefix.prefix_len() < 32 {
            push_expression(
                &mut expressions,
                "bitwise",
                &prefix_mask_expression(route.prefix.netmask().octets()),
            );
        }
        push_expression(
            &mut expressions,
            "cmp",
            &comparison_expression(&route.prefix.network().octets()),
        );
        push_expression(&mut expressions, "masq", &[]);

        let mut attributes = Vec::new();
        push_string_attribute(&mut attributes, NFTA_RULE_TABLE, table_name);
        push_string_attribute(&mut attributes, NFTA_RULE_CHAIN, NFT_CHAIN_NAME);
        push_nested_attribute(&mut attributes, NFTA_RULE_EXPRESSIONS, &expressions);
        push_attribute(&mut attributes, NFTA_RULE_USERDATA, NFT_OWNER_MARKER);
        self.request(
            NFT_MSG_NEWRULE,
            NLM_F_CREATE | NLM_F_APPEND,
            &attributes,
            false,
        )
    }

    fn delete_table(&mut self, table_name: &str, missing_ok: bool) -> Result<()> {
        let mut attributes = Vec::new();
        push_string_attribute(&mut attributes, NFTA_TABLE_NAME, table_name);
        self.request(NFT_MSG_DELTABLE, 0, &attributes, missing_ok)
    }

    fn request(
        &mut self,
        message_type: u8,
        extra_flags: u16,
        attributes: &[u8],
        missing_ok: bool,
    ) -> Result<()> {
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1).max(1);
        let message = netlink_message(message_type, extra_flags, sequence, attributes);
        let sent = self
            .socket
            .send(&message, 0)
            .map_err(|_| AgentError::Network)?;
        if sent != message.len() {
            return Err(AgentError::Network);
        }
        for _ in 0..4 {
            let mut response = vec![0_u8; 64 * 1024];
            let mut response_buffer = &mut response[..];
            let received = self
                .socket
                .recv(&mut response_buffer, 0)
                .map_err(|_| AgentError::Network)?;
            match parse_ack(&response[..received], sequence) {
                Ok(()) => return Ok(()),
                Err(error) if error == ack_not_found() => {}
                Err(error) if missing_ok && error == libc_errno_not_found() => return Ok(()),
                Err(error) => {
                    eprintln!("xs-agent nftables message_type={message_type} errno={error}");
                    return Err(AgentError::Network);
                }
            }
        }
        Err(AgentError::Network)
    }
}

fn netlink_message(
    message_type: u8,
    extra_flags: u16,
    sequence: u32,
    attributes: &[u8],
) -> Vec<u8> {
    let mut message = raw_netlink_message(
        NFNL_MSG_BATCH_BEGIN,
        NLM_F_REQUEST | NLM_F_ACK,
        0,
        0,
        NFNL_SUBSYS_NFTABLES,
        &[],
    );
    let combined_type = (NFNL_SUBSYS_NFTABLES << 8) | u16::from(message_type);
    message.extend_from_slice(&raw_netlink_message(
        combined_type,
        NLM_F_REQUEST | NLM_F_ACK | extra_flags,
        sequence,
        NFPROTO_IPV4,
        0,
        attributes,
    ));
    message.extend_from_slice(&raw_netlink_message(
        NFNL_MSG_BATCH_END,
        NLM_F_REQUEST,
        sequence.wrapping_add(1),
        0,
        NFNL_SUBSYS_NFTABLES,
        &[],
    ));
    message
}

fn raw_netlink_message(
    message_type: u16,
    flags: u16,
    sequence: u32,
    family: u8,
    resource_id: u16,
    attributes: &[u8],
) -> Vec<u8> {
    let length = NETLINK_HEADER_LENGTH + NFGENMSG_LENGTH + attributes.len();
    let mut message = Vec::with_capacity(length);
    message.extend_from_slice(&u32::try_from(length).unwrap_or(u32::MAX).to_ne_bytes());
    message.extend_from_slice(&message_type.to_ne_bytes());
    message.extend_from_slice(&flags.to_ne_bytes());
    message.extend_from_slice(&sequence.to_ne_bytes());
    message.extend_from_slice(&0_u32.to_ne_bytes());
    message.extend_from_slice(&[family, 0]);
    message.extend_from_slice(&resource_id.to_be_bytes());
    message.extend_from_slice(attributes);
    message
}

fn parse_ack(response: &[u8], sequence: u32) -> std::result::Result<(), i32> {
    let mut offset = 0;
    while offset + NETLINK_HEADER_LENGTH <= response.len() {
        let Some(length_bytes) = response.get(offset..offset + 4) else {
            return Err(-1);
        };
        let Ok(length_bytes) = length_bytes.try_into() else {
            return Err(-1);
        };
        let length = u32::from_ne_bytes(length_bytes) as usize;
        if length < NETLINK_HEADER_LENGTH || offset + length > response.len() {
            return Err(-1);
        }
        let Ok(message_type_bytes) = response[offset + 4..offset + 6].try_into() else {
            return Err(-1);
        };
        let message_type = u16::from_ne_bytes(message_type_bytes);
        let Ok(sequence_bytes) = response[offset + 8..offset + 12].try_into() else {
            return Err(-1);
        };
        let message_sequence = u32::from_ne_bytes(sequence_bytes);
        if message_type == NLMSG_ERROR && message_sequence == sequence {
            if length < NETLINK_HEADER_LENGTH + 4 {
                return Err(-1);
            }
            let Ok(error_bytes) = response
                [offset + NETLINK_HEADER_LENGTH..offset + NETLINK_HEADER_LENGTH + 4]
                .try_into()
            else {
                return Err(-1);
            };
            let error = i32::from_ne_bytes(error_bytes);
            return if error == 0 {
                Ok(())
            } else {
                Err(error.saturating_neg())
            };
        }
        offset += align4(length);
    }
    Err(ack_not_found())
}

const fn ack_not_found() -> i32 {
    i32::MIN
}

const fn libc_errno_not_found() -> i32 {
    2
}

fn meta_expression(key: u32) -> Vec<u8> {
    let mut attributes = Vec::new();
    push_u32_attribute(&mut attributes, NFTA_META_DREG, NFT_REGISTER_1);
    push_u32_attribute(&mut attributes, NFTA_META_KEY, key);
    attributes
}

fn comparison_expression(value: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    push_attribute(&mut data, NFTA_DATA_VALUE, value);
    let mut attributes = Vec::new();
    push_u32_attribute(&mut attributes, NFTA_CMP_SREG, NFT_REGISTER_1);
    push_u32_attribute(&mut attributes, NFTA_CMP_OP, NFT_CMP_EQ);
    push_nested_attribute(&mut attributes, NFTA_CMP_DATA, &data);
    attributes
}

fn ipv4_destination_expression() -> Vec<u8> {
    let mut attributes = Vec::new();
    push_u32_attribute(&mut attributes, NFTA_PAYLOAD_DREG, NFT_REGISTER_1);
    push_u32_attribute(
        &mut attributes,
        NFTA_PAYLOAD_BASE,
        NFT_PAYLOAD_NETWORK_HEADER,
    );
    push_u32_attribute(
        &mut attributes,
        NFTA_PAYLOAD_OFFSET,
        IPV4_DESTINATION_OFFSET,
    );
    push_u32_attribute(&mut attributes, NFTA_PAYLOAD_LEN, 4);
    attributes
}

fn prefix_mask_expression(mask: [u8; 4]) -> Vec<u8> {
    let mut mask_data = Vec::new();
    push_attribute(&mut mask_data, NFTA_DATA_VALUE, &mask);
    let mut xor_data = Vec::new();
    push_attribute(&mut xor_data, NFTA_DATA_VALUE, &[0_u8; 4]);
    let mut attributes = Vec::new();
    push_u32_attribute(&mut attributes, NFTA_BITWISE_SREG, NFT_REGISTER_1);
    push_u32_attribute(&mut attributes, NFTA_BITWISE_DREG, NFT_REGISTER_1);
    push_u32_attribute(&mut attributes, NFTA_BITWISE_LEN, 4);
    push_nested_attribute(&mut attributes, NFTA_BITWISE_MASK, &mask_data);
    push_nested_attribute(&mut attributes, NFTA_BITWISE_XOR, &xor_data);
    attributes
}

fn interface_name_bytes(interface_name: &str) -> Vec<u8> {
    let mut bytes = interface_name.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn push_expression(target: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut expression = Vec::new();
    push_string_attribute(&mut expression, NFTA_EXPR_NAME, name);
    push_nested_attribute(&mut expression, NFTA_EXPR_DATA, data);
    push_nested_attribute(target, NFTA_LIST_ELEM, &expression);
}

fn push_u32_attribute(target: &mut Vec<u8>, attribute_type: u16, value: u32) {
    push_attribute(target, attribute_type, &value.to_be_bytes());
}

fn push_i32_attribute(target: &mut Vec<u8>, attribute_type: u16, value: i32) {
    push_attribute(target, attribute_type, &value.to_be_bytes());
}

fn push_string_attribute(target: &mut Vec<u8>, attribute_type: u16, value: &str) {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    push_attribute(target, attribute_type, &bytes);
}

fn push_nested_attribute(target: &mut Vec<u8>, attribute_type: u16, value: &[u8]) {
    push_attribute(target, attribute_type | NLA_F_NESTED, value);
}

fn push_attribute(target: &mut Vec<u8>, attribute_type: u16, value: &[u8]) {
    let length = 4 + value.len();
    let aligned_length = align4(length);
    target.extend_from_slice(&u16::try_from(length).unwrap_or(u16::MAX).to_ne_bytes());
    target.extend_from_slice(&attribute_type.to_ne_bytes());
    target.extend_from_slice(value);
    target.resize(target.len() + aligned_length - length, 0);
}

const fn align4(length: usize) -> usize {
    (length + 3) & !3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netlink_attributes_are_aligned_and_nested() {
        let mut attributes = Vec::new();
        push_string_attribute(&mut attributes, 1, "abc");
        assert_eq!(attributes.len(), 8);
        assert_eq!(u16::from_ne_bytes([attributes[0], attributes[1]]), 8);
        let mut nested = Vec::new();
        push_nested_attribute(&mut nested, 2, &attributes);
        assert_eq!(u16::from_ne_bytes([nested[2], nested[3]]), 2 | NLA_F_NESTED);
    }

    #[test]
    fn nft_rule_contains_prefix_and_interface_expressions() {
        let route = GatewayRoute {
            prefix: "192.168.20.0/24".parse().expect("prefix"),
            interface_name: "eth0".to_owned(),
            mode: SubnetRouteMode::Nat,
        };
        assert_eq!(route.prefix.netmask().octets(), [255, 255, 255, 0]);
        assert_eq!(interface_name_bytes("xsn0"), b"xsn0\0");
    }
}
