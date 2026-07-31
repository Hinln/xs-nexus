#ifndef XSNET_ABI_H
#define XSNET_ABI_H

#include <stddef.h>
#include <stdint.h>

#define XSNET_ABI_MAGIC UINT32_C(0x314E5358)
#define XSNET_ABI_VERSION UINT16_C(1)
#define XSNET_ABI_HEADER_SIZE UINT16_C(32)
#define XSNET_ABI_MAX_PAYLOAD UINT32_C(1048576)
#define XSNET_ABI_MAX_PACKETS UINT16_C(64)
#define XSNET_ABI_MIN_PACKET_SIZE UINT32_C(20)
#define XSNET_ABI_MAX_PACKET_SIZE UINT32_C(9000)

typedef enum XsnetMessageType {
    XSNET_MESSAGE_HELLO = 1,
    XSNET_MESSAGE_ATTACH = 2,
    XSNET_MESSAGE_SET_LINK = 3,
    XSNET_MESSAGE_TX_BATCH = 4,
    XSNET_MESSAGE_RX_BATCH = 5,
    XSNET_MESSAGE_DETACH = 6
} XsnetMessageType;

typedef enum XsnetValidationStatus {
    XSNET_VALID = 0,
    XSNET_INVALID_ARGUMENT = 1,
    XSNET_TRUNCATED = 2,
    XSNET_BAD_MAGIC = 3,
    XSNET_UNSUPPORTED_VERSION = 4,
    XSNET_BAD_HEADER = 5,
    XSNET_BAD_TYPE = 6,
    XSNET_BAD_FLAGS = 7,
    XSNET_BAD_LENGTH = 8,
    XSNET_BAD_SEQUENCE = 9,
    XSNET_BAD_BATCH = 10
} XsnetValidationStatus;

typedef struct XsnetMessageView {
    const uint8_t *payload;
    uint32_t payload_length;
    uint64_t sequence;
} XsnetMessageView;

XsnetValidationStatus XsnetValidateMessage(
    const void *buffer,
    size_t buffer_length,
    XsnetMessageType expected_type,
    uint32_t maximum_payload_length,
    XsnetMessageView *message_view);

XsnetValidationStatus XsnetValidatePacketBatch(
    const uint8_t *payload,
    uint32_t payload_length,
    uint16_t *packet_count);

#endif
