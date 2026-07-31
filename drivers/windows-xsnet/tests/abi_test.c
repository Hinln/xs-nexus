#include "xsnet_abi.h"

#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

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

static void make_message(uint8_t *buffer, uint32_t payload_length) {
    memset(buffer, 0, XSNET_ABI_HEADER_SIZE + payload_length);
    write_u32(buffer, XSNET_ABI_MAGIC);
    write_u16(buffer + 4, XSNET_ABI_VERSION);
    write_u16(buffer + 6, XSNET_ABI_HEADER_SIZE);
    write_u32(buffer + 8, XSNET_MESSAGE_TX_BATCH);
    write_u32(buffer + 16, payload_length);
    write_u64(buffer + 24, UINT64_C(7));
}

static void test_message_validation(void) {
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + 64];
    XsnetMessageView message_view;

    make_message(buffer, 64);
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_VALID);
    assert(message_view.payload_length == 64);
    assert(message_view.sequence == 7);
    assert(XsnetValidateMessage(buffer, 31, XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_TRUNCATED);

    buffer[0] ^= 1;
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_BAD_MAGIC);
    buffer[0] ^= 1;
    write_u16(buffer + 4, 2);
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_UNSUPPORTED_VERSION);
    write_u16(buffer + 4, XSNET_ABI_VERSION);
    write_u16(buffer + 6, 31);
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_BAD_HEADER);
    write_u16(buffer + 6, XSNET_ABI_HEADER_SIZE);
    write_u32(buffer + 12, 1);
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_BAD_FLAGS);
    write_u32(buffer + 12, 0);
    write_u32(buffer + 16, 65);
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_BAD_LENGTH);
    write_u32(buffer + 16, 64);
    write_u64(buffer + 24, 0);
    assert(XsnetValidateMessage(buffer, sizeof(buffer), XSNET_MESSAGE_TX_BATCH, 64, &message_view) == XSNET_BAD_SEQUENCE);
}

static void make_batch(uint8_t *payload) {
    memset(payload, 0, 64);
    write_u16(payload, 2);
    write_u32(payload + 4, 16);
    write_u32(payload + 8, 24);
    write_u32(payload + 12, 20);
    write_u32(payload + 16, 44);
    write_u32(payload + 20, 20);
}

static void test_packet_batch_validation(void) {
    uint8_t payload[64];
    uint16_t packet_count = 0;

    make_batch(payload);
    assert(XsnetValidatePacketBatch(payload, sizeof(payload), &packet_count) == XSNET_VALID);
    assert(packet_count == 2);
    write_u16(payload, 0);
    assert(XsnetValidatePacketBatch(payload, sizeof(payload), &packet_count) == XSNET_BAD_BATCH);
    make_batch(payload);
    write_u32(payload + 4, 8);
    assert(XsnetValidatePacketBatch(payload, sizeof(payload), &packet_count) == XSNET_BAD_BATCH);
    make_batch(payload);
    write_u32(payload + 8, 25);
    assert(XsnetValidatePacketBatch(payload, sizeof(payload), &packet_count) == XSNET_BAD_BATCH);
    make_batch(payload);
    write_u32(payload + 12, 19);
    assert(XsnetValidatePacketBatch(payload, sizeof(payload), &packet_count) == XSNET_BAD_BATCH);
    make_batch(payload);
    assert(XsnetValidatePacketBatch(payload, 63, &packet_count) == XSNET_BAD_BATCH);
}

static void test_message_header_writer(void) {
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + 64];
    XsnetMessageView message_view;

    memset(buffer, 0xa5, sizeof(buffer));
    assert(XsnetWriteMessageHeader(
               buffer,
               sizeof(buffer),
               XSNET_MESSAGE_TX_BATCH,
               64,
               UINT64_C(9)) == XSNET_VALID);
    assert(XsnetValidateMessage(
               buffer,
               sizeof(buffer),
               XSNET_MESSAGE_TX_BATCH,
               64,
               &message_view) == XSNET_VALID);
    assert(message_view.sequence == 9);
    assert(XsnetWriteMessageHeader(
               buffer,
               XSNET_ABI_HEADER_SIZE + 63,
               XSNET_MESSAGE_TX_BATCH,
               64,
               UINT64_C(9)) == XSNET_TRUNCATED);
    assert(XsnetWriteMessageHeader(
               buffer,
               sizeof(buffer),
               XSNET_MESSAGE_TX_BATCH,
               64,
               0) == XSNET_INVALID_ARGUMENT);
}

int main(void) {
    test_message_validation();
    test_packet_batch_validation();
    test_message_header_writer();
    puts("xsnet ABI validation tests passed");
    return 0;
}
