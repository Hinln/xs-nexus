#include "xsnet_driver.h"

static void initialize_queue_context(
    NETPACKETQUEUE packet_queue,
    XsnetQueueDirection direction) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);

    queue_context->direction = direction;
    queue_context->started = FALSE;
    queue_context->notification_enabled = FALSE;
    queue_context->cancelled = FALSE;
}

static void initialize_queue_config(NET_PACKET_QUEUE_CONFIG *queue_config) {
    NET_PACKET_QUEUE_CONFIG_INIT(
        queue_config,
        XsnetEvtPacketQueueAdvance,
        XsnetEvtPacketQueueSetNotificationEnabled,
        XsnetEvtPacketQueueCancel);
    queue_config->EvtStart = XsnetEvtPacketQueueStart;
    queue_config->EvtStop = XsnetEvtPacketQueueStop;
}

NTSTATUS XsnetEvtAdapterCreateTxQueue(
    NETADAPTER adapter,
    NETTXQUEUE_INIT *queue_init) {
    NET_PACKET_QUEUE_CONFIG queue_config;
    WDF_OBJECT_ATTRIBUTES queue_attributes;
    NETPACKETQUEUE packet_queue;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(adapter);
    initialize_queue_config(&queue_config);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&queue_attributes, XsnetQueueContext);
    status = NetTxQueueCreate(
        queue_init,
        &queue_attributes,
        &queue_config,
        &packet_queue);
    if (NT_SUCCESS(status)) {
        initialize_queue_context(packet_queue, XSNET_QUEUE_TRANSMIT);
    }
    return status;
}

NTSTATUS XsnetEvtAdapterCreateRxQueue(
    NETADAPTER adapter,
    NETRXQUEUE_INIT *queue_init) {
    NET_PACKET_QUEUE_CONFIG queue_config;
    WDF_OBJECT_ATTRIBUTES queue_attributes;
    NETPACKETQUEUE packet_queue;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(adapter);
    initialize_queue_config(&queue_config);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&queue_attributes, XsnetQueueContext);
    status = NetRxQueueCreate(
        queue_init,
        &queue_attributes,
        &queue_config,
        &packet_queue);
    if (NT_SUCCESS(status)) {
        initialize_queue_context(packet_queue, XSNET_QUEUE_RECEIVE);
    }
    return status;
}

void XsnetEvtPacketQueueStart(NETPACKETQUEUE packet_queue) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);

    queue_context->cancelled = FALSE;
    queue_context->started = TRUE;
}

void XsnetEvtPacketQueueStop(NETPACKETQUEUE packet_queue) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);

    queue_context->started = FALSE;
    queue_context->notification_enabled = FALSE;
}

void XsnetEvtPacketQueueAdvance(NETPACKETQUEUE packet_queue) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);

    if (!queue_context->started || queue_context->cancelled) {
        return;
    }
}

void XsnetEvtPacketQueueSetNotificationEnabled(
    NETPACKETQUEUE packet_queue,
    BOOLEAN notification_enabled) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);

    queue_context->notification_enabled = notification_enabled;
}

void XsnetEvtPacketQueueCancel(NETPACKETQUEUE packet_queue) {
    XsnetQueueContext *queue_context = XsnetGetQueueContext(packet_queue);

    queue_context->cancelled = TRUE;
    queue_context->started = FALSE;
    queue_context->notification_enabled = FALSE;
}
