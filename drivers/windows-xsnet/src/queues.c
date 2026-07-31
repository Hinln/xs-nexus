#include "xsnet_driver.h"

static void initialize_queue_config(NET_PACKET_QUEUE_CONFIG *queue_config) {
    NET_PACKET_QUEUE_CONFIG_INIT(
        queue_config,
        XsnetEvtPacketQueueAdvance,
        XsnetEvtPacketQueueSetNotificationEnabled,
        XsnetEvtPacketQueueCancel);
    queue_config->EvtStart = XsnetEvtPacketQueueStart;
    queue_config->EvtStop = XsnetEvtPacketQueueStop;
}

static void initialize_queue_context(
    XsnetQueueContext *queue_context,
    WDFDEVICE device,
    XsnetQueueDirection direction,
    const NET_RING_COLLECTION *rings) {
    queue_context->direction = direction;
    queue_context->device = device;
    queue_context->rings = rings;
    queue_context->started = FALSE;
    queue_context->notification_enabled = FALSE;
    queue_context->cancelled = FALSE;
}

static void initialize_virtual_address_query(
    NET_EXTENSION_QUERY *extension_query) {
    NET_EXTENSION_QUERY_INIT(
        extension_query,
        NET_FRAGMENT_EXTENSION_VIRTUAL_ADDRESS_NAME,
        NET_FRAGMENT_EXTENSION_VIRTUAL_ADDRESS_VERSION_1,
        NetExtensionTypeFragment);
}

static NTSTATUS register_packet_queue(
    NETPACKETQUEUE packet_queue,
    WDFDEVICE device,
    XsnetQueueDirection direction) {
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    NTSTATUS status;

    status = WdfWaitLockAcquire(device_context->session_lock, NULL);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    if (direction == XSNET_QUEUE_TRANSMIT) {
        if (device_context->transmit_queue != NULL) {
            status = STATUS_DEVICE_BUSY;
        } else {
            device_context->transmit_queue = packet_queue;
        }
    } else if (device_context->receive_queue != NULL) {
        status = STATUS_DEVICE_BUSY;
    } else {
        device_context->receive_queue = packet_queue;
    }
    WdfWaitLockRelease(device_context->session_lock);
    return status;
}

NTSTATUS XsnetEvtAdapterCreateTxQueue(
    NETADAPTER adapter,
    NETTXQUEUE_INIT *queue_init) {
    NET_PACKET_QUEUE_CONFIG queue_config;
    NET_EXTENSION_QUERY extension_query;
    WDF_OBJECT_ATTRIBUTES queue_attributes;
    WDFDEVICE device = (WDFDEVICE)WdfObjectGetParentObject(adapter);
    NETPACKETQUEUE packet_queue;
    XsnetQueueContext *queue_context;
    NTSTATUS status;

    initialize_queue_config(&queue_config);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&queue_attributes, XsnetQueueContext);
    queue_attributes.ExecutionLevel = WdfExecutionLevelPassive;
    queue_attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    status = NetTxQueueCreate(
        queue_init,
        &queue_attributes,
        &queue_config,
        &packet_queue);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    queue_context = XsnetGetQueueContext(packet_queue);
    initialize_queue_context(
        queue_context,
        device,
        XSNET_QUEUE_TRANSMIT,
        NetTxQueueGetRingCollection(packet_queue));
    initialize_virtual_address_query(&extension_query);
    NetTxQueueGetExtension(
        packet_queue,
        &extension_query,
        &queue_context->virtual_address_extension);
    if (queue_context->rings == NULL ||
        !queue_context->virtual_address_extension.Enabled) {
        WdfObjectDelete(packet_queue);
        return STATUS_NOT_SUPPORTED;
    }
    status = register_packet_queue(
        packet_queue,
        device,
        XSNET_QUEUE_TRANSMIT);
    if (!NT_SUCCESS(status)) {
        WdfObjectDelete(packet_queue);
    }
    return status;
}

NTSTATUS XsnetEvtAdapterCreateRxQueue(
    NETADAPTER adapter,
    NETRXQUEUE_INIT *queue_init) {
    NET_PACKET_QUEUE_CONFIG queue_config;
    NET_EXTENSION_QUERY extension_query;
    WDF_OBJECT_ATTRIBUTES queue_attributes;
    WDFDEVICE device = (WDFDEVICE)WdfObjectGetParentObject(adapter);
    NETPACKETQUEUE packet_queue;
    XsnetQueueContext *queue_context;
    NTSTATUS status;

    initialize_queue_config(&queue_config);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&queue_attributes, XsnetQueueContext);
    queue_attributes.ExecutionLevel = WdfExecutionLevelPassive;
    queue_attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    status = NetRxQueueCreate(
        queue_init,
        &queue_attributes,
        &queue_config,
        &packet_queue);
    if (!NT_SUCCESS(status)) {
        return status;
    }
    queue_context = XsnetGetQueueContext(packet_queue);
    initialize_queue_context(
        queue_context,
        device,
        XSNET_QUEUE_RECEIVE,
        NetRxQueueGetRingCollection(packet_queue));
    initialize_virtual_address_query(&extension_query);
    NetRxQueueGetExtension(
        packet_queue,
        &extension_query,
        &queue_context->virtual_address_extension);
    if (queue_context->rings == NULL ||
        !queue_context->virtual_address_extension.Enabled) {
        WdfObjectDelete(packet_queue);
        return STATUS_NOT_SUPPORTED;
    }
    status = register_packet_queue(
        packet_queue,
        device,
        XSNET_QUEUE_RECEIVE);
    if (!NT_SUCCESS(status)) {
        WdfObjectDelete(packet_queue);
    }
    return status;
}

void XsnetEvtPacketQueueStart(NETPACKETQUEUE packet_queue) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);
    XsnetDeviceContext *device_context =
        XsnetGetDeviceContext(queue_context->device);

    if (NT_SUCCESS(WdfWaitLockAcquire(device_context->session_lock, NULL))) {
        queue_context->cancelled = FALSE;
        queue_context->started = TRUE;
        WdfWaitLockRelease(device_context->session_lock);
    }
}

static BOOLEAN copy_transmit_packets(
    XsnetQueueContext *queue_context,
    XsnetDeviceContext *device_context) {
    NET_RING *packet_ring =
        NetRingCollectionGetPacketRing(queue_context->rings);
    NET_RING *fragment_ring =
        NetRingCollectionGetFragmentRing(queue_context->rings);
    UINT32 packet_index = packet_ring->NextIndex;
    UINT32 fragment_index = fragment_ring->NextIndex;
    BOOLEAN valid = TRUE;

    while (packet_index != packet_ring->EndIndex) {
        NET_PACKET *packet = NetRingGetPacketAtIndex(packet_ring, packet_index);
        NET_FRAGMENT *fragment;
        NET_FRAGMENT_VIRTUAL_ADDRESS *virtual_address;
        XsnetQueueStatus queue_status;

        if (device_context->transmit_packets.count >=
            device_context->session.transmit_queue_depth) {
            break;
        }
        if (packet->Ignore) {
            packet_index = NetRingIncrementIndex(packet_ring, packet_index);
            continue;
        }
        if (packet->FragmentCount != 1 ||
            packet->FragmentIndex >= fragment_ring->NumberOfElements ||
            packet->FragmentIndex != fragment_index) {
            valid = FALSE;
            break;
        }
        fragment_index = packet->FragmentIndex;
        fragment = NetRingGetFragmentAtIndex(fragment_ring, fragment_index);
        virtual_address = NetExtensionGetFragmentVirtualAddress(
            &queue_context->virtual_address_extension,
            fragment_index);
        if (virtual_address == NULL || virtual_address->VirtualAddress == NULL ||
            fragment->Offset > fragment->Capacity ||
            fragment->ValidLength > fragment->Capacity - fragment->Offset) {
            valid = FALSE;
            break;
        }
        queue_status = XsnetPacketQueuePush(
            &device_context->transmit_packets,
            (const uint8_t *)virtual_address->VirtualAddress + fragment->Offset,
            (uint32_t)fragment->ValidLength);
        if (queue_status == XSNET_QUEUE_FULL) {
            break;
        }
        if (queue_status != XSNET_QUEUE_OK) {
            valid = FALSE;
            break;
        }
        fragment_index = NetRingIncrementIndex(fragment_ring, fragment_index);
        packet_index = NetRingIncrementIndex(packet_ring, packet_index);
    }
    packet_ring->NextIndex = packet_index;
    fragment_ring->NextIndex = fragment_index;
    packet_ring->BeginIndex = packet_index;
    fragment_ring->BeginIndex = fragment_index;
    return valid;
}

static BOOLEAN copy_receive_packets(
    XsnetQueueContext *queue_context,
    XsnetDeviceContext *device_context) {
    NET_RING *packet_ring =
        NetRingCollectionGetPacketRing(queue_context->rings);
    NET_RING *fragment_ring =
        NetRingCollectionGetFragmentRing(queue_context->rings);
    UINT32 packet_index = packet_ring->BeginIndex;
    UINT32 fragment_index = fragment_ring->BeginIndex;
    BOOLEAN valid = TRUE;

    fragment_ring->NextIndex = fragment_ring->EndIndex;
    while (device_context->receive_packets.count > 0 &&
           packet_index != packet_ring->EndIndex &&
           fragment_index != fragment_ring->NextIndex) {
        XsnetPacketSlot *slot = &device_context->receive_packets.slots[
            device_context->receive_packets.head];
        NET_FRAGMENT *fragment =
            NetRingGetFragmentAtIndex(fragment_ring, fragment_index);
        NET_FRAGMENT_VIRTUAL_ADDRESS *virtual_address =
            NetExtensionGetFragmentVirtualAddress(
                &queue_context->virtual_address_extension,
                fragment_index);
        NET_PACKET *packet;
        uint32_t packet_length;

        if (virtual_address == NULL || virtual_address->VirtualAddress == NULL ||
            slot->length > fragment->Capacity ||
            XsnetPacketQueuePop(
                &device_context->receive_packets,
                virtual_address->VirtualAddress,
                (uint32_t)fragment->Capacity,
                &packet_length) != XSNET_QUEUE_OK) {
            valid = FALSE;
            break;
        }
        fragment->Offset = 0;
        fragment->ValidLength = packet_length;
        packet = NetRingGetPacketAtIndex(packet_ring, packet_index);
        packet->FragmentIndex = fragment_index;
        packet->FragmentCount = 1;
        packet->Layout.Layer2HeaderLength = 0;
        packet->Layout.Layer3HeaderLength =
            ((const uint8_t *)virtual_address->VirtualAddress)[0] & UINT8_C(0x0f);
        packet->Layout.Layer3HeaderLength *= 4;
        packet->Layout.Layer4HeaderLength = 0;
        packet->Layout.Layer2Type = NetPacketLayer2TypeNull;
        packet->Layout.Layer3Type =
            packet->Layout.Layer3HeaderLength == XSNET_ABI_MIN_PACKET_SIZE
                ? NetPacketLayer3TypeIPv4NoOptions
                : NetPacketLayer3TypeIPv4WithOptions;
        packet->Layout.Layer4Type = NetPacketLayer4TypeUnspecified;
        packet->Ignore = 0;
        fragment_index = NetRingIncrementIndex(fragment_ring, fragment_index);
        packet_index = NetRingIncrementIndex(packet_ring, packet_index);
    }
    fragment_ring->BeginIndex = fragment_index;
    packet_ring->BeginIndex = packet_index;
    return valid;
}

void XsnetEvtPacketQueueAdvance(NETPACKETQUEUE packet_queue) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);
    XsnetDeviceContext *device_context =
        XsnetGetDeviceContext(queue_context->device);
    BOOLEAN valid = TRUE;

    if (!NT_SUCCESS(WdfWaitLockAcquire(device_context->session_lock, NULL))) {
        return;
    }
    if (queue_context->started && !queue_context->cancelled &&
        device_context->session.state == XSNET_SESSION_LINK_UP) {
        if (queue_context->direction == XSNET_QUEUE_TRANSMIT) {
            valid = copy_transmit_packets(
                queue_context,
                device_context);
        } else {
            valid = copy_receive_packets(
                queue_context,
                device_context);
        }
    }
    WdfWaitLockRelease(device_context->session_lock);
    if (!valid) {
        XsnetAdapterSetConnected(queue_context->device, FALSE);
    }
}

void XsnetEvtPacketQueueSetNotificationEnabled(
    NETPACKETQUEUE packet_queue,
    BOOLEAN notification_enabled) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);
    XsnetDeviceContext *device_context =
        XsnetGetDeviceContext(queue_context->device);

    if (NT_SUCCESS(WdfWaitLockAcquire(device_context->session_lock, NULL))) {
        queue_context->notification_enabled = notification_enabled;
        WdfWaitLockRelease(device_context->session_lock);
    }
}

static void reset_packet_queue(
    NETPACKETQUEUE packet_queue,
    BOOLEAN cancelled) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);
    XsnetDeviceContext *device_context =
        XsnetGetDeviceContext(queue_context->device);

    if (NT_SUCCESS(WdfWaitLockAcquire(device_context->session_lock, NULL))) {
        queue_context->started = FALSE;
        queue_context->notification_enabled = FALSE;
        queue_context->cancelled = cancelled;
        if (queue_context->direction == XSNET_QUEUE_TRANSMIT) {
            XsnetPacketQueueReset(&device_context->transmit_packets);
            if (cancelled && device_context->transmit_queue == packet_queue) {
                device_context->transmit_queue = NULL;
            }
        } else {
            XsnetPacketQueueReset(&device_context->receive_packets);
            if (cancelled && device_context->receive_queue == packet_queue) {
                device_context->receive_queue = NULL;
            }
        }
        if (device_context->session.state == XSNET_SESSION_LINK_UP) {
            device_context->session.state = XSNET_SESSION_ATTACHED;
        }
        WdfWaitLockRelease(device_context->session_lock);
    }
    XsnetAdapterSetConnected(queue_context->device, FALSE);
}

void XsnetEvtPacketQueueStop(NETPACKETQUEUE packet_queue) {
    reset_packet_queue(packet_queue, FALSE);
}

void XsnetEvtPacketQueueCancel(NETPACKETQUEUE packet_queue) {
    reset_packet_queue(packet_queue, TRUE);
}
