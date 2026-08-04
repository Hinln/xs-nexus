#include "xsnet_driver.h"

static const UCHAR XSNET_LOCAL_LINK_ADDRESS[6] = {
    0x02, 0x58, 0x53, 0x4e, 0x00, 0x01};

static void XsnetEvtSetReceiveFilter(
    NETADAPTER adapter,
    NETRECEIVEFILTER receive_filter) {
    UNREFERENCED_PARAMETER(adapter);
    UNREFERENCED_PARAMETER(receive_filter);
}

NTSTATUS XsnetAdapterCreate(
    WDFDEVICE device,
    NETADAPTER *adapter) {
    NETADAPTER_INIT *adapter_init;
    NET_ADAPTER_DATAPATH_CALLBACKS datapath_callbacks;
    WDF_OBJECT_ATTRIBUTES adapter_attributes;
    XsnetAdapterContext *adapter_context;
    NTSTATUS status;

    if (adapter == NULL) {
        return STATUS_INVALID_PARAMETER;
    }
    adapter_init = NetAdapterInitAllocate(device);
    if (adapter_init == NULL) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    NET_ADAPTER_DATAPATH_CALLBACKS_INIT(
        &datapath_callbacks,
        XsnetEvtAdapterCreateTxQueue,
        XsnetEvtAdapterCreateRxQueue);
    NetAdapterInitSetDatapathCallbacks(adapter_init, &datapath_callbacks);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(
        &adapter_attributes,
        XsnetAdapterContext);
    status = NetAdapterCreate(adapter_init, &adapter_attributes, adapter);
    NetAdapterInitFree(adapter_init);
    if (NT_SUCCESS(status)) {
        adapter_context = XsnetGetAdapterContext(*adapter);
        adapter_context->device = device;
    }
    return status;
}

NTSTATUS XsnetAdapterStart(WDFDEVICE device) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    NET_ADAPTER_TX_CAPABILITIES transmit_capabilities;
    NET_ADAPTER_RX_CAPABILITIES receive_capabilities;
    NET_ADAPTER_LINK_LAYER_CAPABILITIES link_capabilities;
    NET_ADAPTER_RECEIVE_FILTER_CAPABILITIES receive_filter_capabilities;
    NET_ADAPTER_LINK_LAYER_ADDRESS link_layer_address;
    NET_ADAPTER_LINK_STATE link_state;
    NTSTATUS status;

    if (device_context->adapter == NULL) {
        return STATUS_DEVICE_CONFIGURATION_ERROR;
    }
    if (device_context->adapter_started) {
        return STATUS_SUCCESS;
    }
    NET_ADAPTER_TX_CAPABILITIES_INIT(&transmit_capabilities, 1);
    transmit_capabilities.MaximumNumberOfFragments = 1;
    transmit_capabilities.FragmentBufferAlignment = 1;
    transmit_capabilities.FragmentRingNumberOfElementsHint = 64;
    NET_ADAPTER_RX_CAPABILITIES_INIT_SYSTEM_MANAGED(
        &receive_capabilities,
        XSNET_MAX_FRAME_SIZE,
        1);
    receive_capabilities.FragmentRingNumberOfElementsHint = 64;
    NET_ADAPTER_LINK_LAYER_CAPABILITIES_INIT(
        &link_capabilities,
        XSNET_LINK_SPEED,
        XSNET_LINK_SPEED);
    NET_ADAPTER_RECEIVE_FILTER_CAPABILITIES_INIT(
        &receive_filter_capabilities,
        XsnetEvtSetReceiveFilter);
    receive_filter_capabilities.SupportedPacketFilters =
        NetPacketFilterFlagDirected |
        NetPacketFilterFlagMulticast |
        NetPacketFilterFlagAllMulticast |
        NetPacketFilterFlagBroadcast |
        NetPacketFilterFlagPromiscuous;
    receive_filter_capabilities.MaximumMulticastAddresses = 32;
    NET_ADAPTER_LINK_STATE_INIT_DISCONNECTED(&link_state);
    NET_ADAPTER_LINK_LAYER_ADDRESS_INIT(
        &link_layer_address,
        (USHORT)sizeof(XSNET_LOCAL_LINK_ADDRESS),
        XSNET_LOCAL_LINK_ADDRESS);
    NetAdapterSetDataPathCapabilities(
        device_context->adapter,
        &transmit_capabilities,
        &receive_capabilities);
    NetAdapterSetLinkLayerCapabilities(
        device_context->adapter,
        &link_capabilities);
    NetAdapterSetReceiveFilterCapabilities(
        device_context->adapter,
        &receive_filter_capabilities);
    NetAdapterSetPermanentLinkLayerAddress(
        device_context->adapter,
        &link_layer_address);
    NetAdapterSetCurrentLinkLayerAddress(
        device_context->adapter,
        &link_layer_address);
    NetAdapterSetLinkLayerMtuSize(
        device_context->adapter,
        XSNET_ABI_MAX_PACKET_SIZE);
    NetAdapterSetLinkState(device_context->adapter, &link_state);
    status = NetAdapterStart(device_context->adapter);
    if (NT_SUCCESS(status)) {
        device_context->adapter_started = TRUE;
    }
    return status;
}

void XsnetAdapterStop(WDFDEVICE device) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);

    if (!device_context->adapter_started) {
        return;
    }
    NetAdapterStop(device_context->adapter);
    device_context->adapter_started = FALSE;
}

void XsnetAdapterSetConnected(
    WDFDEVICE device,
    BOOLEAN connected) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    NET_ADAPTER_LINK_STATE link_state;

    if (!device_context->adapter_started) {
        return;
    }
    if (connected) {
        NET_ADAPTER_LINK_STATE_INIT(
            &link_state,
            XSNET_LINK_SPEED,
            MediaConnectStateConnected,
            MediaDuplexStateFull,
            NetAdapterPauseFunctionTypeUnsupported,
            NetAdapterAutoNegotiationFlagNone);
    } else {
        NET_ADAPTER_LINK_STATE_INIT_DISCONNECTED(&link_state);
    }
    NetAdapterSetLinkState(device_context->adapter, &link_state);
}
