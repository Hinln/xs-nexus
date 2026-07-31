#include "xsnet_dataplane.h"

#include <string.h>

static uint32_t read_u32(const uint8_t *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
           ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static uint16_t read_be_u16(const uint8_t *bytes) {
    return (uint16_t)(((uint16_t)bytes[0] << 8) | (uint16_t)bytes[1]);
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

static XsnetQueueStatus validate_ipv4_packet(
    const uint8_t *packet,
    uint32_t packet_length,
    uint32_t mtu) {
    uint32_t header_length;

    if (packet == NULL || packet_length < XSNET_ABI_MIN_PACKET_SIZE ||
        packet_length > mtu) {
        return XSNET_QUEUE_BAD_PACKET;
    }
    header_length = (uint32_t)(packet[0] & UINT8_C(0x0f)) * UINT32_C(4);
    if ((packet[0] >> 4) != 4 || header_length < XSNET_ABI_MIN_PACKET_SIZE ||
        header_length > packet_length ||
        read_be_u16(packet + 2) != packet_length) {
        return XSNET_QUEUE_BAD_PACKET;
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
    if (validate_ipv4_packet(packet, packet_length, queue->mtu) !=
        XSNET_QUEUE_OK) {
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

XsnetQueueStatus XsnetPacketQueueValidateBatch(
    const XsnetPacketQueue *queue,
    const uint8_t *payload,
    uint32_t payload_length,
    uint16_t *packet_count) {
    XsnetValidationStatus validation_status;
    uint16_t validated_packets;
    uint16_t packet_index;

    if (validate_queue(queue) != XSNET_QUEUE_OK || payload == NULL ||
        packet_count == NULL) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    *packet_count = 0;
    validation_status = XsnetValidatePacketBatchForMtu(
        payload,
        payload_length,
        queue->mtu,
        &validated_packets);
    if (validation_status != XSNET_VALID) {
        return XSNET_QUEUE_BAD_PACKET;
    }
    for (packet_index = 0; packet_index < validated_packets; ++packet_index) {
        const uint8_t *descriptor =
            payload + 8 + ((uint32_t)packet_index * UINT32_C(8));
        uint32_t packet_offset = read_u32(descriptor);
        uint32_t packet_length = read_u32(descriptor + 4);

        if (validate_ipv4_packet(
                payload + packet_offset,
                packet_length,
                queue->mtu) != XSNET_QUEUE_OK) {
            return XSNET_QUEUE_BAD_PACKET;
        }
    }
    if (validated_packets > XSNET_ABI_MAX_PACKETS - queue->count) {
        return XSNET_QUEUE_FULL;
    }
    *packet_count = validated_packets;
    return XSNET_QUEUE_OK;
}

XsnetQueueStatus XsnetPacketQueuePushBatch(
    XsnetPacketQueue *queue,
    const uint8_t *payload,
    uint32_t payload_length,
    uint16_t *accepted_packets) {
    uint16_t packet_count;
    uint16_t packet_index;
    XsnetQueueStatus validation_status;

    if (accepted_packets == NULL) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    *accepted_packets = 0;
    validation_status = XsnetPacketQueueValidateBatch(
        queue,
        payload,
        payload_length,
        &packet_count);
    if (validation_status != XSNET_QUEUE_OK) {
        return validation_status;
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

XsnetQueueStatus XsnetPacketQueuePop(
    XsnetPacketQueue *queue,
    void *packet,
    uint32_t packet_capacity,
    uint32_t *packet_length) {
    XsnetPacketSlot *slot;

    if (validate_queue(queue) != XSNET_QUEUE_OK || packet == NULL ||
        packet_length == NULL) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    *packet_length = 0;
    if (queue->count == 0) {
        return XSNET_QUEUE_EMPTY;
    }
    slot = &queue->slots[queue->head];
    if (packet_capacity < slot->length) {
        return XSNET_QUEUE_BUFFER_TOO_SMALL;
    }
    memcpy(packet, slot->bytes, slot->length);
    *packet_length = slot->length;
    secure_zero(slot, sizeof(*slot));
    queue->head = queue_index(queue, 1);
    queue->count = (uint16_t)(queue->count - 1);
    return XSNET_QUEUE_OK;
}

XsnetQueueStatus XsnetPacketQueueMeasureBatch(
    const XsnetPacketQueue *queue,
    uint32_t payload_capacity,
    uint16_t maximum_packets,
    uint32_t *payload_length,
    uint16_t *packet_count) {
    uint16_t selected_packets;
    uint32_t selected_bytes;

    if (validate_queue(queue) != XSNET_QUEUE_OK || payload_length == NULL ||
        packet_count == NULL ||
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
    *payload_length = UINT32_C(8) +
                      ((uint32_t)selected_packets * UINT32_C(8)) +
                      selected_bytes;
    *packet_count = selected_packets;
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
    uint32_t selected_length;
    uint32_t data_offset;
    uint16_t packet_index;
    XsnetQueueStatus queue_status;

    if (payload == NULL || payload_length == NULL || packet_count == NULL) {
        return XSNET_QUEUE_INVALID_ARGUMENT;
    }
    *payload_length = 0;
    *packet_count = 0;
    queue_status = XsnetPacketQueueMeasureBatch(
        queue,
        payload_capacity,
        maximum_packets,
        &selected_length,
        &selected_packets);
    if (queue_status != XSNET_QUEUE_OK) {
        return queue_status;
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
    *payload_length = selected_length;
    *packet_count = selected_packets;
    return XSNET_QUEUE_OK;
}
