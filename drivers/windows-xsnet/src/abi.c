#include "xsnet_abi.h"

static uint16_t read_u16(const uint8_t *bytes) {
    return (uint16_t)((uint16_t)bytes[0] | ((uint16_t)bytes[1] << 8));
}

static uint32_t read_u32(const uint8_t *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
           ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static uint64_t read_u64(const uint8_t *bytes) {
    return (uint64_t)read_u32(bytes) | ((uint64_t)read_u32(bytes + 4) << 32);
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

static void write_u64(uint8_t *bytes, uint64_t value) {
    write_u32(bytes, (uint32_t)value);
    write_u32(bytes + 4, (uint32_t)(value >> 32));
}

XsnetValidationStatus XsnetValidateMessage(
    const void *buffer,
    size_t buffer_length,
    XsnetMessageType expected_type,
    uint32_t maximum_payload_length,
    XsnetMessageView *message_view) {
    const uint8_t *bytes = (const uint8_t *)buffer;
    uint32_t payload_length;
    size_t total_length;

    if (buffer == NULL || message_view == NULL ||
        maximum_payload_length > XSNET_ABI_MAX_PAYLOAD) {
        return XSNET_INVALID_ARGUMENT;
    }
    if (buffer_length < XSNET_ABI_HEADER_SIZE) {
        return XSNET_TRUNCATED;
    }
    if (read_u32(bytes) != XSNET_ABI_MAGIC) {
        return XSNET_BAD_MAGIC;
    }
    if (read_u16(bytes + 4) != XSNET_ABI_VERSION) {
        return XSNET_UNSUPPORTED_VERSION;
    }
    if (read_u16(bytes + 6) != XSNET_ABI_HEADER_SIZE || read_u32(bytes + 20) != 0) {
        return XSNET_BAD_HEADER;
    }
    if (read_u32(bytes + 8) != (uint32_t)expected_type) {
        return XSNET_BAD_TYPE;
    }
    if (read_u32(bytes + 12) != 0) {
        return XSNET_BAD_FLAGS;
    }
    payload_length = read_u32(bytes + 16);
    if (payload_length > maximum_payload_length) {
        return XSNET_BAD_LENGTH;
    }
    total_length = (size_t)XSNET_ABI_HEADER_SIZE + (size_t)payload_length;
    if (total_length != buffer_length) {
        return XSNET_BAD_LENGTH;
    }
    if (read_u64(bytes + 24) == 0) {
        return XSNET_BAD_SEQUENCE;
    }

    message_view->payload = bytes + XSNET_ABI_HEADER_SIZE;
    message_view->payload_length = payload_length;
    message_view->sequence = read_u64(bytes + 24);
    return XSNET_VALID;
}

XsnetValidationStatus XsnetWriteMessageHeader(
    void *buffer,
    size_t buffer_capacity,
    XsnetMessageType message_type,
    uint32_t payload_length,
    uint64_t sequence) {
    uint8_t *bytes = (uint8_t *)buffer;

    if (buffer == NULL || message_type < XSNET_MESSAGE_HELLO ||
        message_type > XSNET_MESSAGE_DETACH ||
        payload_length > XSNET_ABI_MAX_PAYLOAD || sequence == 0) {
        return XSNET_INVALID_ARGUMENT;
    }
    if (buffer_capacity <
        (size_t)XSNET_ABI_HEADER_SIZE + (size_t)payload_length) {
        return XSNET_TRUNCATED;
    }
    write_u32(bytes, XSNET_ABI_MAGIC);
    write_u16(bytes + 4, XSNET_ABI_VERSION);
    write_u16(bytes + 6, XSNET_ABI_HEADER_SIZE);
    write_u32(bytes + 8, (uint32_t)message_type);
    write_u32(bytes + 12, 0);
    write_u32(bytes + 16, payload_length);
    write_u32(bytes + 20, 0);
    write_u64(bytes + 24, sequence);
    return XSNET_VALID;
}

XsnetValidationStatus XsnetValidatePacketBatch(
    const uint8_t *payload,
    uint32_t payload_length,
    uint16_t *packet_count) {
    return XsnetValidatePacketBatchForMtu(
        payload,
        payload_length,
        XSNET_ABI_MAX_PACKET_SIZE,
        packet_count);
}

XsnetValidationStatus XsnetValidatePacketBatchForMtu(
    const uint8_t *payload,
    uint32_t payload_length,
    uint32_t maximum_packet_size,
    uint16_t *packet_count) {
    uint16_t count;
    uint32_t descriptor_length;
    uint32_t expected_offset;
    uint16_t packet_index;

    if (payload == NULL || packet_count == NULL ||
        maximum_packet_size < XSNET_ABI_MIN_PACKET_SIZE ||
        maximum_packet_size > XSNET_ABI_MAX_PACKET_SIZE) {
        return XSNET_INVALID_ARGUMENT;
    }
    if (payload_length < 8) {
        return XSNET_BAD_BATCH;
    }
    count = read_u16(payload);
    if (count == 0 || count > XSNET_ABI_MAX_PACKETS || read_u16(payload + 2) != 0) {
        return XSNET_BAD_BATCH;
    }
    descriptor_length = read_u32(payload + 4);
    if (descriptor_length != (uint32_t)count * UINT32_C(8) ||
        descriptor_length > payload_length - UINT32_C(8)) {
        return XSNET_BAD_BATCH;
    }
    expected_offset = UINT32_C(8) + descriptor_length;
    for (packet_index = 0; packet_index < count; ++packet_index) {
        const uint8_t *descriptor = payload + 8 + ((uint32_t)packet_index * UINT32_C(8));
        uint32_t packet_offset = read_u32(descriptor);
        uint32_t packet_length = read_u32(descriptor + 4);

        if (packet_offset != expected_offset || packet_length < XSNET_ABI_MIN_PACKET_SIZE ||
            packet_length > maximum_packet_size || packet_length > payload_length - packet_offset) {
            return XSNET_BAD_BATCH;
        }
        expected_offset = packet_offset + packet_length;
    }
    if (expected_offset != payload_length) {
        return XSNET_BAD_BATCH;
    }
    *packet_count = count;
    return XSNET_VALID;
}
