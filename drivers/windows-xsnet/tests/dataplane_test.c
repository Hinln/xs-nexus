#include "xsnet_dataplane.h"

#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

static XsnetPacketSlot test_slots[XSNET_ABI_MAX_PACKETS];

static void fill_packet(uint8_t *packet, uint32_t packet_length, uint8_t value) {
    memset(packet, value, packet_length);
    packet[0] = 0x45;
    packet[2] = (uint8_t)(packet_length >> 8);
    packet[3] = (uint8_t)packet_length;
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

static uint32_t make_two_packet_batch(
    uint8_t *payload,
    uint32_t first_length,
    uint32_t second_length) {
    uint32_t first_offset = 24;
    uint32_t second_offset = first_offset + first_length;

    memset(payload, 0, second_offset + second_length);
    write_u16(payload, 2);
    write_u32(payload + 4, 16);
    write_u32(payload + 8, first_offset);
    write_u32(payload + 12, first_length);
    write_u32(payload + 16, second_offset);
    write_u32(payload + 20, second_length);
    fill_packet(payload + first_offset, first_length, 0x31);
    fill_packet(payload + second_offset, second_length, 0x52);
    return second_offset + second_length;
}

static void test_single_packet_round_trip(void) {
    XsnetPacketQueue queue;
    XsnetValidationStatus validation_status;
    uint8_t packet[64];
    uint8_t payload[128];
    uint32_t payload_length;
    uint16_t packet_count;

    fill_packet(packet, sizeof(packet), 0x27);
    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1400) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePopBatch(
               &queue,
               payload,
               sizeof(payload),
               XSNET_ABI_MAX_PACKETS,
               &payload_length,
               &packet_count) == XSNET_QUEUE_OK);
    assert(packet_count == 1);
    assert(queue.count == 0);
    validation_status = XsnetValidatePacketBatchForMtu(
        payload,
        payload_length,
        1400,
        &packet_count);
    assert(validation_status == XSNET_VALID);
    assert(memcmp(payload + 16, packet, sizeof(packet)) == 0);
    assert(test_slots[0].length == 0);
}

static void test_batch_atomicity_and_mtu(void) {
    XsnetPacketQueue queue;
    uint8_t payload[1400];
    uint32_t payload_length;
    uint16_t accepted_packets;

    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1280) == XSNET_QUEUE_OK);
    payload_length = make_two_packet_batch(payload, 40, 60);
    assert(XsnetPacketQueuePushBatch(
               &queue,
               payload,
               payload_length,
               &accepted_packets) == XSNET_QUEUE_OK);
    assert(accepted_packets == 2);
    assert(queue.count == 2);
    payload_length = make_two_packet_batch(payload, 40, 1281);
    assert(XsnetPacketQueuePushBatch(
               &queue,
               payload,
               payload_length,
               &accepted_packets) == XSNET_QUEUE_BAD_PACKET);
    assert(accepted_packets == 0);
    assert(queue.count == 2);
    payload_length = make_two_packet_batch(payload, 40, 60);
    payload[64] = 0x65;
    assert(XsnetPacketQueuePushBatch(
               &queue,
               payload,
               payload_length,
               &accepted_packets) == XSNET_QUEUE_BAD_PACKET);
    assert(accepted_packets == 0);
    assert(queue.count == 2);
}

static void test_capacity_is_atomic(void) {
    XsnetPacketQueue queue;
    uint8_t packet[20];
    uint8_t payload[80];
    uint32_t payload_length;
    uint16_t accepted_packets;
    uint16_t packet_index;

    fill_packet(packet, sizeof(packet), 0x44);
    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1400) == XSNET_QUEUE_OK);
    for (packet_index = 0; packet_index < XSNET_ABI_MAX_PACKETS - 1; ++packet_index) {
        assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
    }
    payload_length = make_two_packet_batch(payload, 20, 20);
    assert(XsnetPacketQueuePushBatch(
               &queue,
               payload,
               payload_length,
               &accepted_packets) == XSNET_QUEUE_FULL);
    assert(queue.count == XSNET_ABI_MAX_PACKETS - 1);
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_FULL);
}

static void test_small_output_preserves_queue(void) {
    XsnetPacketQueue queue;
    uint8_t packet[32];
    uint8_t payload[47];
    uint32_t payload_length;
    uint16_t packet_count;

    fill_packet(packet, sizeof(packet), 0x13);
    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1400) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueueMeasureBatch(
               &queue,
               sizeof(payload),
               1,
               &payload_length,
               &packet_count) == XSNET_QUEUE_BUFFER_TOO_SMALL);
    assert(queue.count == 1);
    assert(XsnetPacketQueuePopBatch(
               &queue,
               payload,
               sizeof(payload),
               1,
               &payload_length,
               &packet_count) == XSNET_QUEUE_BUFFER_TOO_SMALL);
    assert(queue.count == 1);
    assert(test_slots[0].length == sizeof(packet));
}

static void test_partial_pop_and_wraparound(void) {
    XsnetPacketQueue queue;
    uint8_t packet[20];
    uint8_t payload[64];
    uint32_t payload_length;
    uint16_t packet_count;
    uint16_t iteration;

    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1400) == XSNET_QUEUE_OK);
    for (iteration = 0; iteration < 200; ++iteration) {
        fill_packet(packet, sizeof(packet), (uint8_t)iteration);
        assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
        assert(XsnetPacketQueuePopBatch(
                   &queue,
                   payload,
                   sizeof(payload),
                   1,
                   &payload_length,
                   &packet_count) == XSNET_QUEUE_OK);
        assert(packet_count == 1);
        assert(payload[16] == 0x45);
    }
    assert(queue.count == 0);
    assert(queue.head == (uint16_t)(200 % XSNET_ABI_MAX_PACKETS));
}

static void test_reset_zeroes_payloads(void) {
    XsnetPacketQueue queue;
    uint8_t packet[64];
    size_t byte_index;

    fill_packet(packet, sizeof(packet), 0x7f);
    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1400) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
    XsnetPacketQueueReset(&queue);
    assert(queue.head == 0);
    assert(queue.count == 0);
    for (byte_index = 0; byte_index < sizeof(test_slots); ++byte_index) {
        assert(((const uint8_t *)test_slots)[byte_index] == 0);
    }
}

static void test_ipv4_validation_and_single_pop(void) {
    XsnetPacketQueue queue;
    uint8_t packet[64];
    uint8_t output[64];
    uint32_t output_length;

    fill_packet(packet, sizeof(packet), 0x61);
    assert(XsnetPacketQueueInitialize(&queue, test_slots, 1400) == XSNET_QUEUE_OK);
    packet[0] = 0x65;
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) ==
           XSNET_QUEUE_BAD_PACKET);
    fill_packet(packet, sizeof(packet), 0x61);
    packet[3] -= 1;
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) ==
           XSNET_QUEUE_BAD_PACKET);
    fill_packet(packet, sizeof(packet), 0x61);
    assert(XsnetPacketQueuePush(&queue, packet, sizeof(packet)) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePop(
               &queue,
               output,
               sizeof(output) - 1,
               &output_length) == XSNET_QUEUE_BUFFER_TOO_SMALL);
    assert(queue.count == 1);
    assert(XsnetPacketQueuePop(
               &queue,
               output,
               sizeof(output),
               &output_length) == XSNET_QUEUE_OK);
    assert(output_length == sizeof(packet));
    assert(memcmp(output, packet, sizeof(packet)) == 0);
    assert(queue.count == 0);
}

int main(void) {
    test_single_packet_round_trip();
    test_batch_atomicity_and_mtu();
    test_capacity_is_atomic();
    test_small_output_preserves_queue();
    test_partial_pop_and_wraparound();
    test_reset_zeroes_payloads();
    test_ipv4_validation_and_single_pop();
    puts("xsnet dataplane queue tests passed");
    return 0;
}
