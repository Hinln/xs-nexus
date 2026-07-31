#include "xsnet_abi.h"
#include "xsnet_dataplane.h"
#include "xsnet_session.h"

#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define STRESS_OWNER UINT64_C(0x4f574e4552535452)
#define STRESS_ITERATIONS UINT32_C(30000)

static uint64_t random_state = UINT64_C(0x8d4c3b2a19081706);
static XsnetPacketSlot stress_slots[XSNET_ABI_MAX_PACKETS];
static XsnetPacketSlot stress_snapshot_slots[XSNET_ABI_MAX_PACKETS];

static uint32_t next_random(void) {
    random_state ^= random_state << 13;
    random_state ^= random_state >> 7;
    random_state ^= random_state << 17;
    return (uint32_t)(random_state >> 16);
}

static void fill_random(uint8_t *buffer, size_t length) {
    size_t byte_index;

    for (byte_index = 0; byte_index < length; ++byte_index) {
        buffer[byte_index] = (uint8_t)next_random();
    }
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

static void make_ipv4_packet(
    uint8_t *packet,
    uint32_t packet_length) {
    fill_random(packet, packet_length);
    packet[0] = UINT8_C(0x45);
    packet[2] = (uint8_t)(packet_length >> 8);
    packet[3] = (uint8_t)packet_length;
}

static void make_message(
    uint8_t *buffer,
    XsnetMessageType message_type,
    uint32_t payload_length,
    uint64_t sequence) {
    memset(buffer, 0, XSNET_ABI_HEADER_SIZE + payload_length);
    write_u32(buffer, XSNET_ABI_MAGIC);
    write_u16(buffer + 4, XSNET_ABI_VERSION);
    write_u16(buffer + 6, XSNET_ABI_HEADER_SIZE);
    write_u32(buffer + 8, (uint32_t)message_type);
    write_u32(buffer + 16, payload_length);
    write_u64(buffer + 24, sequence);
}

static int sessions_equal(
    const XsnetSession *left,
    const XsnetSession *right) {
    return left->owner_cookie == right->owner_cookie &&
           left->last_sequence == right->last_sequence &&
           left->negotiated_capabilities == right->negotiated_capabilities &&
           left->mtu == right->mtu &&
           left->transmit_queue_depth == right->transmit_queue_depth &&
           left->receive_queue_depth == right->receive_queue_depth &&
           left->state == right->state;
}

static void assert_queue_shape(const XsnetPacketQueue *queue) {
    uint8_t active_slots[XSNET_ABI_MAX_PACKETS] = {0};
    uint16_t offset;
    uint16_t slot_index;

    assert(queue->head < XSNET_ABI_MAX_PACKETS);
    assert(queue->count <= XSNET_ABI_MAX_PACKETS);
    assert(queue->mtu >= XSNET_ABI_MIN_PACKET_SIZE);
    assert(queue->mtu <= XSNET_ABI_MAX_PACKET_SIZE);
    for (offset = 0; offset < queue->count; ++offset) {
        slot_index =
            (uint16_t)((queue->head + offset) % XSNET_ABI_MAX_PACKETS);
        active_slots[slot_index] = 1;
    }
    for (slot_index = 0; slot_index < XSNET_ABI_MAX_PACKETS; ++slot_index) {
        if (active_slots[slot_index]) {
            const XsnetPacketSlot *slot = &queue->slots[slot_index];

            assert(slot->length >= XSNET_ABI_MIN_PACKET_SIZE);
            assert(slot->length <= queue->mtu);
            assert((slot->bytes[0] >> 4) == 4);
            assert((((uint32_t)slot->bytes[2] << 8) | slot->bytes[3]) ==
                   slot->length);
        } else {
            assert(queue->slots[slot_index].length == 0);
        }
    }
}

static void assert_queue_unchanged(
    const XsnetPacketQueue *queue,
    const XsnetPacketQueue *previous_queue) {
    assert(queue->slots == previous_queue->slots);
    assert(queue->mtu == previous_queue->mtu);
    assert(queue->head == previous_queue->head);
    assert(queue->count == previous_queue->count);
    assert(memcmp(
               queue->slots,
               stress_snapshot_slots,
               sizeof(stress_snapshot_slots)) == 0);
}

static void stress_message_parser(void) {
    uint8_t buffer[4096];
    uint32_t iteration;

    for (iteration = 0; iteration < STRESS_ITERATIONS; ++iteration) {
        size_t buffer_length = next_random() % sizeof(buffer);
        XsnetMessageType message_type =
            (XsnetMessageType)(next_random() % UINT32_C(9));
        uint32_t maximum_payload = next_random() % sizeof(buffer);
        XsnetMessageView message_view;
        XsnetValidationStatus validation_status;

        fill_random(buffer, buffer_length);
        validation_status = XsnetValidateMessage(
            buffer,
            buffer_length,
            message_type,
            maximum_payload,
            &message_view);
        if (validation_status == XSNET_VALID) {
            assert(buffer_length >= XSNET_ABI_HEADER_SIZE);
            assert(message_view.payload == buffer + XSNET_ABI_HEADER_SIZE);
            assert((size_t)message_view.payload_length +
                       XSNET_ABI_HEADER_SIZE ==
                   buffer_length);
            assert(message_view.sequence != 0);
        }
    }
}

static void stress_message_writer(void) {
    uint8_t buffer[XSNET_ABI_HEADER_SIZE + 1024];
    uint32_t iteration;

    for (iteration = 0; iteration < STRESS_ITERATIONS; ++iteration) {
        XsnetMessageType message_type =
            (XsnetMessageType)(1 + (next_random() % XSNET_MESSAGE_DETACH));
        uint32_t payload_length = next_random() % UINT32_C(1025);
        uint64_t sequence = ((uint64_t)next_random() << 32) | next_random();
        XsnetMessageView message_view;

        if (sequence == 0) {
            sequence = 1;
        }
        fill_random(buffer, sizeof(buffer));
        assert(XsnetWriteMessageHeader(
                   buffer,
                   sizeof(buffer),
                   message_type,
                   payload_length,
                   sequence) == XSNET_VALID);
        assert(XsnetValidateMessage(
                   buffer,
                   XSNET_ABI_HEADER_SIZE + payload_length,
                   message_type,
                   payload_length,
                   &message_view) == XSNET_VALID);
        assert(message_view.payload_length == payload_length);
        assert(message_view.sequence == sequence);
    }
}

static void stress_batch_parser(void) {
    uint8_t payload[4096];
    uint32_t iteration;

    for (iteration = 0; iteration < STRESS_ITERATIONS; ++iteration) {
        uint32_t payload_length = next_random() % sizeof(payload);
        uint32_t maximum_packet_size = next_random() % UINT32_C(10000);
        uint16_t packet_count;
        XsnetValidationStatus validation_status;

        fill_random(payload, payload_length);
        validation_status = XsnetValidatePacketBatchForMtu(
            payload,
            payload_length,
            maximum_packet_size,
            &packet_count);
        if (validation_status == XSNET_VALID) {
            assert(packet_count > 0);
            assert(packet_count <= XSNET_ABI_MAX_PACKETS);
            assert(maximum_packet_size >= XSNET_ABI_MIN_PACKET_SIZE);
            assert(maximum_packet_size <= XSNET_ABI_MAX_PACKET_SIZE);
        }
    }
}

static void stress_session_atomicity(void) {
    uint8_t buffer[1024];
    XsnetSession session;
    uint32_t iteration;

    XsnetSessionInitialize(&session);
    assert(XsnetSessionOpen(&session, STRESS_OWNER) == XSNET_SESSION_OK);
    for (iteration = 0; iteration < STRESS_ITERATIONS; ++iteration) {
        size_t buffer_length = next_random() % sizeof(buffer);
        XsnetMessageType message_type =
            (XsnetMessageType)(next_random() % UINT32_C(9));
        uint64_t owner = (next_random() & 1) ? STRESS_OWNER : next_random();
        XsnetValidationStatus validation_status;
        XsnetSession previous_session = session;
        XsnetSessionStatus session_status;

        fill_random(buffer, buffer_length);
        session_status = XsnetSessionProcess(
            &session,
            owner,
            message_type,
            buffer,
            buffer_length,
            &validation_status);
        if (session_status != XSNET_SESSION_OK) {
            assert(sessions_equal(&session, &previous_session));
        } else {
            XsnetSessionInitialize(&session);
            assert(XsnetSessionOpen(&session, STRESS_OWNER) == XSNET_SESSION_OK);
        }
    }
}

static void assert_message_mutations_atomic(
    const XsnetSession *base_session,
    XsnetMessageType message_type,
    const uint8_t *message,
    size_t message_length) {
    uint8_t mutated[2048];
    size_t byte_index;

    assert(message_length <= sizeof(mutated));
    for (byte_index = 0; byte_index < message_length; ++byte_index) {
        XsnetSession session = *base_session;
        XsnetSession previous_session = session;
        XsnetValidationStatus validation_status;
        XsnetSessionStatus session_status;

        memcpy(mutated, message, message_length);
        mutated[byte_index] ^= UINT8_C(0x80);
        session_status = XsnetSessionProcess(
            &session,
            STRESS_OWNER,
            message_type,
            mutated,
            message_length,
            &validation_status);
        if (session_status != XSNET_SESSION_OK) {
            assert(sessions_equal(&session, &previous_session));
        }
    }
}

static void stress_valid_session_mutations(void) {
    uint8_t message[2048];
    XsnetSession session;
    uint32_t payload_length;
    size_t message_length;

    XsnetSessionInitialize(&session);
    assert(XsnetSessionOpen(&session, STRESS_OWNER) == XSNET_SESSION_OK);
    make_message(
        message,
        XSNET_MESSAGE_HELLO,
        XSNET_ABI_HELLO_PAYLOAD_SIZE,
        1);
    write_u16(message + XSNET_ABI_HEADER_SIZE, XSNET_ABI_VERSION);
    write_u16(message + XSNET_ABI_HEADER_SIZE + 2, XSNET_ABI_VERSION);
    write_u32(
        message + XSNET_ABI_HEADER_SIZE + 4,
        XSNET_CAPABILITY_IPV4);
    assert_message_mutations_atomic(
        &session,
        XSNET_MESSAGE_HELLO,
        message,
        XSNET_ABI_HEADER_SIZE + XSNET_ABI_HELLO_PAYLOAD_SIZE);

    session.last_sequence = 1;
    session.negotiated_capabilities = XSNET_CAPABILITY_IPV4;
    session.state = XSNET_SESSION_NEGOTIATED;
    make_message(
        message,
        XSNET_MESSAGE_ATTACH,
        XSNET_ABI_ATTACH_PAYLOAD_SIZE,
        2);
    write_u32(message + XSNET_ABI_HEADER_SIZE, 1400);
    write_u16(message + XSNET_ABI_HEADER_SIZE + 4, 32);
    write_u16(message + XSNET_ABI_HEADER_SIZE + 6, 32);
    write_u32(
        message + XSNET_ABI_HEADER_SIZE + 8,
        XSNET_CAPABILITY_IPV4);
    assert_message_mutations_atomic(
        &session,
        XSNET_MESSAGE_ATTACH,
        message,
        XSNET_ABI_HEADER_SIZE + XSNET_ABI_ATTACH_PAYLOAD_SIZE);

    session.last_sequence = 2;
    session.mtu = 1400;
    session.transmit_queue_depth = 32;
    session.receive_queue_depth = 32;
    session.state = XSNET_SESSION_ATTACHED;
    make_message(
        message,
        XSNET_MESSAGE_SET_LINK,
        XSNET_ABI_SET_LINK_PAYLOAD_SIZE,
        3);
    write_u32(message + XSNET_ABI_HEADER_SIZE, 1);
    assert_message_mutations_atomic(
        &session,
        XSNET_MESSAGE_SET_LINK,
        message,
        XSNET_ABI_HEADER_SIZE + XSNET_ABI_SET_LINK_PAYLOAD_SIZE);

    session.last_sequence = 3;
    session.state = XSNET_SESSION_LINK_UP;
    make_message(message, XSNET_MESSAGE_TX_BATCH, 0, 4);
    assert_message_mutations_atomic(
        &session,
        XSNET_MESSAGE_TX_BATCH,
        message,
        XSNET_ABI_HEADER_SIZE);

    payload_length = UINT32_C(16) + XSNET_ABI_MIN_PACKET_SIZE;
    make_message(message, XSNET_MESSAGE_RX_BATCH, payload_length, 4);
    write_u16(message + XSNET_ABI_HEADER_SIZE, 1);
    write_u32(message + XSNET_ABI_HEADER_SIZE + 4, 8);
    write_u32(message + XSNET_ABI_HEADER_SIZE + 8, 16);
    write_u32(
        message + XSNET_ABI_HEADER_SIZE + 12,
        XSNET_ABI_MIN_PACKET_SIZE);
    message_length = XSNET_ABI_HEADER_SIZE + payload_length;
    assert_message_mutations_atomic(
        &session,
        XSNET_MESSAGE_RX_BATCH,
        message,
        message_length);

    make_message(message, XSNET_MESSAGE_DETACH, 0, 4);
    assert_message_mutations_atomic(
        &session,
        XSNET_MESSAGE_DETACH,
        message,
        XSNET_ABI_HEADER_SIZE);
}

static void stress_packet_queue(void) {
    XsnetPacketQueue queue;
    uint8_t packet[512];
    uint8_t payload[4096];
    uint32_t iteration;

    assert(XsnetPacketQueueInitialize(&queue, stress_slots, 1400) ==
           XSNET_QUEUE_OK);
    for (iteration = 0; iteration < STRESS_ITERATIONS; ++iteration) {
        uint32_t action = next_random() % UINT32_C(6);
        XsnetPacketQueue previous_queue = queue;
        uint16_t previous_count = queue.count;

        memcpy(
            stress_snapshot_slots,
            queue.slots,
            sizeof(stress_snapshot_slots));

        if (action == 0) {
            uint32_t packet_length =
                XSNET_ABI_MIN_PACKET_SIZE + (next_random() % UINT32_C(493));
            XsnetQueueStatus queue_status;

            make_ipv4_packet(packet, packet_length);
            queue_status = XsnetPacketQueuePush(
                &queue,
                packet,
                packet_length);
            if (previous_count == XSNET_ABI_MAX_PACKETS) {
                assert(queue_status == XSNET_QUEUE_FULL);
                assert_queue_unchanged(&queue, &previous_queue);
            } else {
                assert(queue_status == XSNET_QUEUE_OK);
                assert(queue.count == previous_count + 1);
            }
        } else if (action == 1) {
            uint32_t packet_length = next_random() % sizeof(packet);
            XsnetQueueStatus queue_status;

            fill_random(packet, packet_length);
            queue_status = XsnetPacketQueuePush(
                &queue,
                packet,
                packet_length);
            if (queue_status != XSNET_QUEUE_OK) {
                assert_queue_unchanged(&queue, &previous_queue);
            }
        } else if (action == 2) {
            uint32_t packet_capacity = next_random() % sizeof(packet);
            uint32_t packet_length;
            XsnetQueueStatus queue_status = XsnetPacketQueuePop(
                &queue,
                packet,
                packet_capacity,
                &packet_length);

            if (queue_status == XSNET_QUEUE_OK) {
                assert(previous_count > 0);
                assert(queue_status == XSNET_QUEUE_OK);
                assert(packet_length >= XSNET_ABI_MIN_PACKET_SIZE);
                assert(packet_length <= packet_capacity);
                assert(queue.count == previous_count - 1);
            } else {
                assert(queue_status == XSNET_QUEUE_EMPTY ||
                       queue_status == XSNET_QUEUE_BUFFER_TOO_SMALL);
                assert_queue_unchanged(&queue, &previous_queue);
            }
        } else if (action == 3) {
            uint32_t payload_capacity = next_random() % sizeof(payload);
            uint16_t maximum_packets =
                (uint16_t)(1 + (next_random() % XSNET_ABI_MAX_PACKETS));
            uint32_t payload_length;
            uint16_t packet_count;
            XsnetQueueStatus queue_status = XsnetPacketQueuePopBatch(
                &queue,
                payload,
                payload_capacity,
                maximum_packets,
                &payload_length,
                &packet_count);

            if (queue_status == XSNET_QUEUE_OK) {
                uint16_t validated_packets;

                assert(packet_count > 0);
                assert(queue.count == previous_count - packet_count);
                assert(XsnetValidatePacketBatchForMtu(
                           payload,
                           payload_length,
                           queue.mtu,
                           &validated_packets) == XSNET_VALID);
                assert(validated_packets == packet_count);
            } else {
                assert_queue_unchanged(&queue, &previous_queue);
            }
        } else if (action == 4) {
            uint32_t payload_length;
            uint16_t packet_count;

            (void)XsnetPacketQueueMeasureBatch(
                &queue,
                next_random() % sizeof(payload),
                (uint16_t)(1 + (next_random() % XSNET_ABI_MAX_PACKETS)),
                &payload_length,
                &packet_count);
            assert_queue_unchanged(&queue, &previous_queue);
        } else {
            XsnetPacketQueueReset(&queue);
            assert(queue.count == 0);
            assert(queue.head == 0);
        }
        assert_queue_shape(&queue);
    }
}

int main(void) {
    stress_message_parser();
    stress_message_writer();
    stress_batch_parser();
    stress_session_atomicity();
    stress_valid_session_mutations();
    stress_packet_queue();
    puts("xsnet deterministic stress tests passed");
    return 0;
}
