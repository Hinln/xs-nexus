#include "xsnet_session.h"

#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define TEST_OWNER UINT64_C(0x1122334455667788)

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

static size_t make_message(
    uint8_t *buffer,
    XsnetMessageType message_type,
    uint64_t sequence,
    uint32_t payload_length) {
    memset(buffer, 0, XSNET_ABI_HEADER_SIZE + payload_length);
    write_u32(buffer, XSNET_ABI_MAGIC);
    write_u16(buffer + 4, XSNET_ABI_VERSION);
    write_u16(buffer + 6, XSNET_ABI_HEADER_SIZE);
    write_u32(buffer + 8, (uint32_t)message_type);
    write_u32(buffer + 16, payload_length);
    write_u64(buffer + 24, sequence);
    return (size_t)XSNET_ABI_HEADER_SIZE + payload_length;
}

static size_t make_hello(uint8_t *buffer, uint64_t sequence) {
    size_t message_length = make_message(
        buffer,
        XSNET_MESSAGE_HELLO,
        sequence,
        XSNET_ABI_HELLO_PAYLOAD_SIZE);
    uint8_t *payload = buffer + XSNET_ABI_HEADER_SIZE;

    write_u16(payload, XSNET_ABI_VERSION);
    write_u16(payload + 2, XSNET_ABI_VERSION);
    write_u32(payload + 4, XSNET_CAPABILITY_IPV4);
    return message_length;
}

static size_t make_attach(uint8_t *buffer, uint64_t sequence, uint32_t mtu) {
    size_t message_length = make_message(
        buffer,
        XSNET_MESSAGE_ATTACH,
        sequence,
        XSNET_ABI_ATTACH_PAYLOAD_SIZE);
    uint8_t *payload = buffer + XSNET_ABI_HEADER_SIZE;

    write_u32(payload, mtu);
    write_u16(payload + 4, 32);
    write_u16(payload + 6, 32);
    write_u32(payload + 8, XSNET_CAPABILITY_IPV4);
    return message_length;
}

static size_t make_set_link(uint8_t *buffer, uint64_t sequence, uint32_t link_up) {
    size_t message_length = make_message(
        buffer,
        XSNET_MESSAGE_SET_LINK,
        sequence,
        XSNET_ABI_SET_LINK_PAYLOAD_SIZE);

    write_u32(buffer + XSNET_ABI_HEADER_SIZE, link_up);
    return message_length;
}

static size_t make_batch(
    uint8_t *buffer,
    XsnetMessageType message_type,
    uint64_t sequence,
    uint32_t packet_length) {
    uint32_t payload_length = UINT32_C(16) + packet_length;
    size_t message_length = make_message(
        buffer,
        message_type,
        sequence,
        payload_length);
    uint8_t *payload = buffer + XSNET_ABI_HEADER_SIZE;

    write_u16(payload, 1);
    write_u32(payload + 4, 8);
    write_u32(payload + 8, 16);
    write_u32(payload + 12, packet_length);
    return message_length;
}

static void negotiate_and_attach(
    XsnetSession *session,
    uint8_t *buffer,
    XsnetValidationStatus *validation_status) {
    size_t message_length;

    assert(XsnetSessionOpen(session, TEST_OWNER) == XSNET_SESSION_OK);
    message_length = make_hello(buffer, 1);
    assert(XsnetSessionProcess(
               session,
               TEST_OWNER,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               validation_status) == XSNET_SESSION_OK);
    message_length = make_attach(buffer, 2, 1400);
    assert(XsnetSessionProcess(
               session,
               TEST_OWNER,
               XSNET_MESSAGE_ATTACH,
               buffer,
               message_length,
               validation_status) == XSNET_SESSION_OK);
}

static void test_owner_lifecycle(void) {
    XsnetSession session;

    XsnetSessionInitialize(&session);
    assert(session.state == XSNET_SESSION_CLOSED);
    assert(XsnetSessionOpen(&session, 0) == XSNET_SESSION_INVALID_ARGUMENT);
    assert(XsnetSessionOpen(&session, TEST_OWNER) == XSNET_SESSION_OK);
    assert(XsnetSessionOpen(&session, TEST_OWNER + 1) == XSNET_SESSION_BAD_STATE);
    assert(XsnetSessionClose(&session, TEST_OWNER + 1) == XSNET_SESSION_BAD_OWNER);
    assert(session.state == XSNET_SESSION_OPENED);
    assert(XsnetSessionClose(&session, TEST_OWNER) == XSNET_SESSION_OK);
    assert(session.state == XSNET_SESSION_CLOSED);
    assert(session.owner_cookie == 0);
}

static void test_negotiation_rejects_invalid_inputs(void) {
    XsnetSession session;
    XsnetValidationStatus validation_status;
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + XSNET_ABI_HELLO_PAYLOAD_SIZE];
    size_t message_length;

    XsnetSessionInitialize(&session);
    assert(XsnetSessionOpen(&session, TEST_OWNER) == XSNET_SESSION_OK);
    message_length = make_hello(buffer, 1);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER + 1,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_OWNER);
    write_u16(buffer + XSNET_ABI_HEADER_SIZE, 2);
    write_u16(buffer + XSNET_ABI_HEADER_SIZE + 2, 3);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_VERSION_RANGE);
    assert(session.last_sequence == 0);
    message_length = make_hello(buffer, 1);
    write_u32(buffer + XSNET_ABI_HEADER_SIZE + 4, 0);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_CAPABILITIES);
    assert(session.state == XSNET_SESSION_OPENED);
}

static void test_order_sequence_and_attach_limits(void) {
    XsnetSession session;
    XsnetValidationStatus validation_status;
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + XSNET_ABI_ATTACH_PAYLOAD_SIZE];
    size_t message_length;

    XsnetSessionInitialize(&session);
    assert(XsnetSessionOpen(&session, TEST_OWNER) == XSNET_SESSION_OK);
    message_length = make_attach(buffer, 1, 1400);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_ATTACH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_STATE);
    message_length = make_hello(buffer, 1);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_SEQUENCE);
    message_length = make_attach(buffer, 2, XSNET_SESSION_MIN_MTU - 1);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_ATTACH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_MTU);
    write_u32(buffer + XSNET_ABI_HEADER_SIZE, 1400);
    write_u16(buffer + XSNET_ABI_HEADER_SIZE + 4, 0);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_ATTACH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_QUEUE_DEPTH);
    message_length = make_attach(buffer, 2, 1400);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_ATTACH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    assert(session.state == XSNET_SESSION_ATTACHED);
    assert(session.mtu == 1400);
}

static void test_link_packet_and_detach(void) {
    XsnetSession session;
    XsnetValidationStatus validation_status;
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + 16 + 1401];
    size_t message_length;

    XsnetSessionInitialize(&session);
    negotiate_and_attach(&session, buffer, &validation_status);
    message_length = make_message(buffer, XSNET_MESSAGE_TX_BATCH, 3, 0);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_TX_BATCH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_STATE);
    message_length = make_set_link(buffer, 3, 1);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_SET_LINK,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    assert(session.state == XSNET_SESSION_LINK_UP);
    message_length = make_batch(buffer, XSNET_MESSAGE_TX_BATCH, 4, 20);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_TX_BATCH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_MESSAGE);
    assert(validation_status == XSNET_BAD_LENGTH);
    assert(session.last_sequence == 3);
    message_length = make_message(buffer, XSNET_MESSAGE_TX_BATCH, 4, 0);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_TX_BATCH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    message_length = make_batch(buffer, XSNET_MESSAGE_RX_BATCH, 5, 1400);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_RX_BATCH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    message_length = make_message(buffer, XSNET_MESSAGE_DETACH, 6, 0);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_DETACH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    assert(session.state == XSNET_SESSION_NEGOTIATED);
    message_length = make_message(buffer, XSNET_MESSAGE_DETACH, 7, 0);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_DETACH,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_OK);
    assert(XsnetSessionClose(&session, TEST_OWNER) == XSNET_SESSION_OK);
}

static void test_sequence_ceiling_requires_reopen(void) {
    XsnetSession session;
    XsnetValidationStatus validation_status;
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + XSNET_ABI_HELLO_PAYLOAD_SIZE];
    size_t message_length;

    XsnetSessionInitialize(&session);
    assert(XsnetSessionOpen(&session, TEST_OWNER) == XSNET_SESSION_OK);
    message_length = make_hello(buffer, UINT64_MAX);
    assert(XsnetSessionProcess(
               &session,
               TEST_OWNER,
               XSNET_MESSAGE_HELLO,
               buffer,
               message_length,
               &validation_status) == XSNET_SESSION_BAD_SEQUENCE);
    assert(session.state == XSNET_SESSION_OPENED);
    assert(XsnetSessionClose(&session, TEST_OWNER) == XSNET_SESSION_OK);
}

int main(void) {
    test_owner_lifecycle();
    test_negotiation_rejects_invalid_inputs();
    test_order_sequence_and_attach_limits();
    test_link_packet_and_detach();
    test_sequence_ceiling_requires_reopen();
    puts("xsnet session validation tests passed");
    return 0;
}
