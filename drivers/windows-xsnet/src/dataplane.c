#include "xsnet_dataplane.h"

#include <string.h>

static uint32_t read_u32(const uint8_t *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
           ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static void write_u16(uint8_t *bytes, uint16_t value) {
    bytes[0] = (uint8_t)value;
    bytes[1] = (uint8_t)(value >> 8);
}

static void write_u32(uint8_t *bytes, uint32_t value) {
    bytes[0] = (uint8_t)value;
    bytes[1] = (uint8_t)(value >> 8);
    bytes[2] = (uint8_t)(value >> 16);
    bytes[3] = (uint8_t)(value >> 24);
}

static void secure_zero(void *buffer, size_t length) {
    volatile uint8_t *bytes = (volatile uint8_t *)buffer;

    while (length > 0) {
        *bytes = 0;
        ++bytes;
        --length;
    }
}

static uint16_t queue_index(
    const XsnetPacketQueue *queue,
    uint16_t offset) {
    return (uint16_t)((queue->head + offset) % XSNET_ABI_MAX_PACKETS);
}

static XsnetQueueStatus validate_queue(const XsnetPacketQueue *queue) {
    if (queue == NULL || queue->slots == NULL ||
        queue->mtu < XSNET_ABI_MIN_PACKET_SIZE ||
        queue->mtu > XSNET_ABI_MAX_PACKET_SIZE ||
        queue->head >= XSNET_ABI_MAX_PACKETS ||
        queue->count > XSNET_ABI_MAX_PACKETS) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    return XSNET_QUEUE_OK;
}

XsnetQueueStatus XsnetPacketQueueInitialize(
    XsnetPacketQueue *queue,
    XsnetPacketSlot *slots,
    uint32_t mtu) {
    if (queue == NULL || slots == NULL || mtu < XSNET_ABI_MIN_PACKET_SIZE ||
        mtu > XSNET_ABI_MAX_PACKET_SIZE) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    queue->slots = slots;
    queue->mtu = mtu;
    queue->head = 0;
    queue->count = 0;
    secure_zero(slots, sizeof(*slots) * XSNET_ABI_MAX_PACKETS);
    return XSNET_QUEUE_OK;
}

void XsnetPacketQueueReset(XsnetPacketQueue *queue) {
    if (queue == NULL || queue->slots == NULL) {
        return;
    }
    secure_zero(
        queue->slots,
        sizeof(*queue->slots) * XSNET_ABI_MAX_PACKETS);
    queue->head = 0;
    queue->count = 0;
}

XsnetQueueStatus XsnetPacketQueuePush(
    XsnetPacketQueue *queue,
    const void *packet,
    uint32_t packet_length) {
    uint16_t tail;

    if (validate_queue(queue) != XSNET_QUEUE_OK || packet == NULL) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    if (packet_length < XSNET_ABI_MIN_PACKET_SIZE ||
        packet_length > queue->mtu) {
        return XSNET_QUEUE_BAD_PACKET;
    }
    if (queue->count == XSNET_ABI_MAX_PACKETS) {
        return XSNET_QUEUE_FULL;
    }
    tail = queue_index(queue, queue->count);
    memcpy(queue->slots[tail].bytes, packet, packet_length);
    queue->slots[tail].length = packet_length;
    queue->count += 1;
    return XSNET_QUEUE_OK;
}

XsnetQueueStatus XsnetPacketQueuePushBatch(
    XsnetPacketQueue *queue,
    const uint8_t *payload,
    uint32_t payload_length,
    uint16_t *accepted_packets) {
    XsnetValidationStatus validation_status;
    uint16_t packet_count;
    uint16_t packet_index;

    if (validate_queue(queue) != XSNET_QUEUE_OK || payload == NULL ||
        accepted_packets == NULL) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    *accepted_packets = 0;
    validation_status = XsnetValidatePacketBatchForMtu(
        payload,
        payload_length,
        queue->mtu,
        &packet_count);
    if (validation_status != XSNET_VALID) {
        return XSNET_QUEUE_BAD_PACKET;
    }
    if (packet_count > XSNET_ABI_MAX_PACKETS - queue->count) {
        return XSNET_QUEUE_FULL;
    }
    for (packet_index = 0; packet_index < packet_count; ++packet_index) {
        const uint8_t *descriptor =
            payload + 8 + ((uint32_t)packet_index * UINT32_C(8));
        uint32_t packet_offset = read_u32(descriptor);
        uint32_t packet_length = read_u32(descriptor + 4);
        XsnetQueueStatus queue_status = XsnetPacketQueuePush(
            queue,
            payload + packet_offset,
            packet_length);

        if (queue_status != XSNET_QUEUE_OK) {
            return queue_status;
        }
    }
    *accepted_packets = packet_count;
    return XSNET_QUEUE_OK;
}

XsnetQueueStatus XsnetPacketQueuePopBatch(
    XsnetPacketQueue *queue,
    uint8_t *payload,
    uint32_t payload_capacity,
    uint16_t maximum_packets,
    uint32_t *payload_length,
    uint16_t *packet_count) {
    uint16_t selected_packets;
    uint32_t selected_bytes;
    uint32_t data_offset;
    uint16_t packet_index;

    if (validate_queue(queue) != XSNET_QUEUE_OK || payload == NULL ||
        payload_length == NULL || packet_count == NULL ||
        maximum_packets == 0 || maximum_packets > XSNET_ABI_MAX_PACKETS) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    *payload_length = 0;
    *packet_count = 0;
    if (queue->count == 0) {
        return XSNET_QUEUE_EMPTY;
    }

    selected_packets = 0;
    selected_bytes = 0;
    while (selected_packets < queue->count &&
           selected_packets < maximum_packets) {
        uint16_t slot_index = queue_index(queue, selected_packets);
        uint32_t candidate_packets = (uint32_t)selected_packets + 1;
        uint32_t candidate_length = UINT32_C(8) +
                                    (candidate_packets * UINT32_C(8)) +
                                    selected_bytes +
                                    queue->slots[slot_index].length;

        if (candidate_length > payload_capacity) {
            break;
        }
        selected_bytes += queue->slots[slot_index].length;
        selected_packets += 1;
    }
    if (selected_packets == 0) {
        return XSNET_QUEUE_BUFFER_TOO_SMALL;
    }

    write_u16(payload, selected_packets);
    write_u16(payload + 2, 0);
    write_u32(payload + 4, (uint32_t)selected_packets * UINT32_C(8));
    data_offset = UINT32_C(8) +
                  ((uint32_t)selected_packets * UINT32_C(8));
    for (packet_index = 0; packet_index < selected_packets; ++packet_index) {
        uint16_t slot_index = queue_index(queue, packet_index);
        uint8_t *descriptor =
            payload + 8 + ((uint32_t)packet_index * UINT32_C(8));

        write_u32(descriptor, data_offset);
        write_u32(descriptor + 4, queue->slots[slot_index].length);
        memcpy(
            payload + data_offset,
            queue->slots[slot_index].bytes,
            queue->slots[slot_index].length);
        data_offset += queue->slots[slot_index].length;
    }
    for (packet_index = 0; packet_index < selected_packets; ++packet_index) {
        uint16_t slot_index = queue_index(queue, packet_index);

        secure_zero(&queue->slots[slot_index], sizeof(queue->slots[slot_index]));
    }
    queue->head = queue_index(queue, selected_packets);
    queue->count = (uint16_t)(queue->count - selected_packets);
    *payload_length = data_offset;
    *packet_count = selected_packets;
    return XSNET_QUEUE_OK;
}
