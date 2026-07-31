#include "xsnet_identity.h"

#include <string.h>

static uint16_t read_u16_le(const uint8_t *buffer) {
    return (uint16_t)((uint16_t)buffer[0] | ((uint16_t)buffer[1] << 8));
}

static uint32_t read_u32_le(const uint8_t *buffer) {
    return (uint32_t)buffer[0] |
           ((uint32_t)buffer[1] << 8) |
           ((uint32_t)buffer[2] << 16) |
           ((uint32_t)buffer[3] << 24);
}

static void write_u16_le(uint8_t *buffer, uint16_t value) {
    buffer[0] = (uint8_t)value;
    buffer[1] = (uint8_t)(value >> 8);
}

static void write_u64_le(uint8_t *buffer, uint64_t value) {
    uint32_t index;

    for (index = 0; index < 8; ++index) {
        buffer[index] = (uint8_t)(value >> (index * 8));
    }
}

XsnetIdentityStatus XsnetValidateIdentityRequest(
    const void *buffer,
    size_t buffer_length) {
    const uint8_t *bytes = (const uint8_t *)buffer;

    if (buffer == NULL) {
        return XSNET_IDENTITY_INVALID_ARGUMENT;
    }
    if (buffer_length != XSNET_IDENTITY_REQUEST_SIZE) {
        return XSNET_IDENTITY_BAD_LENGTH;
    }
    if (read_u16_le(bytes) != XSNET_IDENTITY_VERSION ||
        read_u16_le(bytes + 2) != XSNET_IDENTITY_REQUEST_SIZE) {
        return XSNET_IDENTITY_BAD_VERSION;
    }
    if (read_u32_le(bytes + 4) != 0) {
        return XSNET_IDENTITY_BAD_RESERVED;
    }
    return XSNET_IDENTITY_VALID;
}

XsnetIdentityStatus XsnetWriteIdentityResponse(
    void *buffer,
    size_t buffer_length,
    uint64_t interface_luid) {
    uint8_t *bytes = (uint8_t *)buffer;

    if (buffer == NULL) {
        return XSNET_IDENTITY_INVALID_ARGUMENT;
    }
    if (buffer_length != XSNET_IDENTITY_RESPONSE_SIZE) {
        return XSNET_IDENTITY_BAD_LENGTH;
    }
    if (interface_luid == 0) {
        return XSNET_IDENTITY_BAD_LUID;
    }
    memset(bytes, 0, buffer_length);
    write_u16_le(bytes, XSNET_IDENTITY_VERSION);
    write_u16_le(bytes + 2, XSNET_IDENTITY_RESPONSE_SIZE);
    write_u64_le(bytes + 8, interface_luid);
    return XSNET_IDENTITY_VALID;
}
