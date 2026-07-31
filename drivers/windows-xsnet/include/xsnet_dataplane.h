#ifndef XSNET_DATAPLANE_H
#define XSNET_DATAPLANE_H

#include "xsnet_abi.h"

#include <stddef.h>
#include <stdint.h>

typedef enum XsnetQueueStatus {
    XSNET_QUEUE_OK = 0,
    XSNET_QUEUE_INVALID_ARGUMENT = 1,
    XSNET_QUEUE_BAD_PACKET = 2,
    XSNET_QUEUE_FULL = 3,
    XSNET_QUEUE_EMPTY = 4,
    XSNET_QUEUE_BUFFER_TOO_SMALL = 5
} XsnetQueueStatus;

typedef struct XsnetPacketSlot {
    uint32_t length;
    uint8_t bytes[XSNET_ABI_MAX_PACKET_SIZE];
} XsnetPacketSlot;

typedef struct XsnetPacketQueue {
    XsnetPacketSlot *slots;
    uint32_t mtu;
    uint16_t head;
    uint16_t count;
} XsnetPacketQueue;

XsnetQueueStatus XsnetPacketQueueInitialize(
    XsnetPacketQueue *queue,
    XsnetPacketSlot *slots,
    uint32_t mtu);

void XsnetPacketQueueReset(XsnetPacketQueue *queue);

XsnetQueueStatus XsnetPacketQueuePush(
    XsnetPacketQueue *queue,
    const void *packet,
    uint32_t packet_length);

XsnetQueueStatus XsnetPacketQueuePushBatch(
    XsnetPacketQueue *queue,
    const uint8_t *payload,
    uint32_t payload_length,
    uint16_t *accepted_packets);

XsnetQueueStatus XsnetPacketQueuePopBatch(
    XsnetPacketQueue *queue,
    uint8_t *payload,
    uint32_t payload_capacity,
    uint16_t maximum_packets,
    uint32_t *payload_length,
    uint16_t *packet_count);

#endif
