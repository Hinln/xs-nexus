#include "xsnet_dataplane.h"
#include "xsnet_session.h"

#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef enum LifecycleEvent {
    LIFECYCLE_CLEANUP = 0,
    LIFECYCLE_TX_CANCEL = 1,
    LIFECYCLE_RX_CANCEL = 2,
    LIFECYCLE_D0_EXIT = 3,
    LIFECYCLE_RELEASE = 4,
    LIFECYCLE_IO_STOP = 5,
    LIFECYCLE_EVENT_COUNT = 6
} LifecycleEvent;

typedef struct LifecycleHarness {
    XsnetSession session;
    XsnetPacketQueue transmit_packets;
    XsnetPacketQueue receive_packets;
    XsnetPacketSlot transmit_slots[XSNET_ABI_MAX_PACKETS];
    XsnetPacketSlot receive_slots[XSNET_ABI_MAX_PACKETS];
    uint64_t file_owner;
    uint32_t requests_started;
    uint32_t requests_completed;
    uint32_t requests_cancelled;
    bool hardware_prepared;
    bool in_d0;
    bool adapter_started;
    bool file_accepted;
    bool transmit_started;
    bool receive_started;
    bool link_connected;
    bool request_active;
} LifecycleHarness;

static LifecycleHarness harness;

static void make_ipv4_packet(uint8_t *packet, uint32_t packet_length) {
    uint32_t byte_index;

    assert(packet_length >= XSNET_ABI_MIN_PACKET_SIZE);
    memset(packet, 0, packet_length);
    packet[0] = UINT8_C(0x45);
    packet[2] = (uint8_t)(packet_length >> 8);
    packet[3] = (uint8_t)packet_length;
    for (byte_index = XSNET_ABI_MIN_PACKET_SIZE;
         byte_index < packet_length;
         ++byte_index) {
        packet[byte_index] = (uint8_t)byte_index;
    }
}

static void reset_session_and_packets(LifecycleHarness *state) {
    XsnetSessionInitialize(&state->session);
    XsnetPacketQueueReset(&state->transmit_packets);
    XsnetPacketQueueReset(&state->receive_packets);
    state->transmit_packets.mtu = XSNET_ABI_MAX_PACKET_SIZE;
    state->receive_packets.mtu = XSNET_ABI_MAX_PACKET_SIZE;
}

static void cancel_active_request(LifecycleHarness *state) {
    if (state->request_active) {
        state->request_active = false;
        state->requests_cancelled += 1;
    }
}

static void disconnect_link(LifecycleHarness *state) {
    state->link_connected = false;
    if (state->session.state == XSNET_SESSION_LINK_UP) {
        state->session.state = XSNET_SESSION_ATTACHED;
    }
}

static void assert_lifecycle_invariants(const LifecycleHarness *state) {
    assert(state->requests_completed + state->requests_cancelled +
               (state->request_active ? 1U : 0U) ==
           state->requests_started);
    assert(state->transmit_packets.count <= XSNET_ABI_MAX_PACKETS);
    assert(state->receive_packets.count <= XSNET_ABI_MAX_PACKETS);
    if (!state->file_accepted) {
        assert(state->session.owner_cookie == 0);
    }
    if (state->session.owner_cookie != 0) {
        assert(state->file_accepted);
        assert(state->session.owner_cookie == state->file_owner);
    }
    if (state->request_active) {
        assert(state->file_accepted);
        assert(state->link_connected);
    }
    if (state->link_connected) {
        assert(state->hardware_prepared);
        assert(state->in_d0);
        assert(state->adapter_started);
        assert(state->file_accepted);
        assert(state->transmit_started);
        assert(state->receive_started);
        assert(state->session.state == XSNET_SESSION_LINK_UP);
    }
}

static void initialize_harness(LifecycleHarness *state) {
    memset(state, 0, sizeof(*state));
    XsnetSessionInitialize(&state->session);
    assert(XsnetPacketQueueInitialize(
               &state->transmit_packets,
               state->transmit_slots,
               XSNET_ABI_MAX_PACKET_SIZE) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueueInitialize(
               &state->receive_packets,
               state->receive_slots,
               XSNET_ABI_MAX_PACKET_SIZE) == XSNET_QUEUE_OK);
    assert_lifecycle_invariants(state);
}

static void prepare_and_enter_d0(LifecycleHarness *state) {
    state->hardware_prepared = true;
    state->adapter_started = true;
    state->in_d0 = true;
    state->link_connected = false;
    assert_lifecycle_invariants(state);
}

static void open_owner(LifecycleHarness *state) {
    assert(state->hardware_prepared);
    assert(state->in_d0);
    assert(state->adapter_started);
    assert(!state->file_accepted);
    state->file_owner += 1;
    if (state->file_owner == 0) {
        state->file_owner += 1;
    }
    assert(XsnetSessionOpen(&state->session, state->file_owner) ==
           XSNET_SESSION_OK);
    state->file_accepted = true;
    XsnetPacketQueueReset(&state->transmit_packets);
    XsnetPacketQueueReset(&state->receive_packets);
    assert_lifecycle_invariants(state);
}

static void activate_harness(LifecycleHarness *state, bool begin_request) {
    uint8_t packet[XSNET_ABI_MIN_PACKET_SIZE];

    initialize_harness(state);
    prepare_and_enter_d0(state);
    open_owner(state);
    state->session.negotiated_capabilities = XSNET_CAPABILITY_IPV4;
    state->session.last_sequence = 3;
    state->session.mtu = 1400;
    state->session.transmit_queue_depth = 32;
    state->session.receive_queue_depth = 32;
    state->session.state = XSNET_SESSION_LINK_UP;
    state->transmit_packets.mtu = 1400;
    state->receive_packets.mtu = 1400;
    state->transmit_started = true;
    state->receive_started = true;
    state->link_connected = true;
    make_ipv4_packet(packet, sizeof(packet));
    assert(XsnetPacketQueuePush(
               &state->transmit_packets,
               packet,
               sizeof(packet)) == XSNET_QUEUE_OK);
    assert(XsnetPacketQueuePush(
               &state->receive_packets,
               packet,
               sizeof(packet)) == XSNET_QUEUE_OK);
    if (begin_request) {
        state->request_active = true;
        state->requests_started += 1;
    }
    assert_lifecycle_invariants(state);
}

static void apply_cleanup(LifecycleHarness *state) {
    if (state->file_accepted) {
        (void)XsnetSessionClose(&state->session, state->file_owner);
        XsnetPacketQueueReset(&state->transmit_packets);
        XsnetPacketQueueReset(&state->receive_packets);
        state->file_accepted = false;
    }
    cancel_active_request(state);
    state->link_connected = false;
}

static void apply_queue_cancel(
    LifecycleHarness *state,
    bool transmit) {
    if (transmit) {
        state->transmit_started = false;
        XsnetPacketQueueReset(&state->transmit_packets);
    } else {
        state->receive_started = false;
        XsnetPacketQueueReset(&state->receive_packets);
    }
    cancel_active_request(state);
    disconnect_link(state);
}

static void apply_d0_exit(LifecycleHarness *state) {
    reset_session_and_packets(state);
    cancel_active_request(state);
    state->in_d0 = false;
    state->transmit_started = false;
    state->receive_started = false;
    state->link_connected = false;
}

static void apply_release(LifecycleHarness *state) {
    reset_session_and_packets(state);
    cancel_active_request(state);
    state->hardware_prepared = false;
    state->in_d0 = false;
    state->adapter_started = false;
    state->transmit_started = false;
    state->receive_started = false;
    state->link_connected = false;
}

static void apply_event(
    LifecycleHarness *state,
    LifecycleEvent event) {
    switch (event) {
        case LIFECYCLE_CLEANUP:
            apply_cleanup(state);
            break;
        case LIFECYCLE_TX_CANCEL:
            apply_queue_cancel(state, true);
            break;
        case LIFECYCLE_RX_CANCEL:
            apply_queue_cancel(state, false);
            break;
        case LIFECYCLE_D0_EXIT:
            apply_d0_exit(state);
            break;
        case LIFECYCLE_RELEASE:
            apply_release(state);
            break;
        case LIFECYCLE_IO_STOP:
            cancel_active_request(state);
            break;
        default:
            assert(false);
    }
    assert_lifecycle_invariants(state);
}

static void complete_active_request(LifecycleHarness *state) {
    assert(state->request_active);
    state->request_active = false;
    state->requests_completed += 1;
    assert_lifecycle_invariants(state);
}

static void validate_teardown_order(const LifecycleEvent *events) {
    uint32_t event_index;

    activate_harness(&harness, true);
    for (event_index = 0;
         event_index < LIFECYCLE_EVENT_COUNT;
         ++event_index) {
        apply_event(&harness, events[event_index]);
    }
    assert(!harness.hardware_prepared);
    assert(!harness.in_d0);
    assert(!harness.adapter_started);
    assert(!harness.file_accepted);
    assert(!harness.transmit_started);
    assert(!harness.receive_started);
    assert(!harness.link_connected);
    assert(!harness.request_active);
    assert(harness.requests_started == 1);
    assert(harness.requests_completed == 0);
    assert(harness.requests_cancelled == 1);
    assert(harness.session.state == XSNET_SESSION_CLOSED);
    assert(harness.transmit_packets.count == 0);
    assert(harness.receive_packets.count == 0);
}

static void permute_teardown_events(
    LifecycleEvent *events,
    uint32_t event_index,
    uint32_t *permutation_count) {
    uint32_t swap_index;

    if (event_index == LIFECYCLE_EVENT_COUNT) {
        validate_teardown_order(events);
        *permutation_count += 1;
        return;
    }
    for (swap_index = event_index;
         swap_index < LIFECYCLE_EVENT_COUNT;
         ++swap_index) {
        LifecycleEvent saved_event = events[event_index];

        events[event_index] = events[swap_index];
        events[swap_index] = saved_event;
        permute_teardown_events(
            events,
            event_index + 1,
            permutation_count);
        saved_event = events[event_index];
        events[event_index] = events[swap_index];
        events[swap_index] = saved_event;
    }
}

static void test_all_teardown_interleavings(void) {
    LifecycleEvent events[LIFECYCLE_EVENT_COUNT] = {
        LIFECYCLE_CLEANUP,
        LIFECYCLE_TX_CANCEL,
        LIFECYCLE_RX_CANCEL,
        LIFECYCLE_D0_EXIT,
        LIFECYCLE_RELEASE,
        LIFECYCLE_IO_STOP,
    };
    uint32_t permutation_count = 0;

    permute_teardown_events(events, 0, &permutation_count);
    assert(permutation_count == 720);
}

static void test_completion_wins_before_teardown(void) {
    activate_harness(&harness, true);
    complete_active_request(&harness);
    apply_event(&harness, LIFECYCLE_CLEANUP);
    apply_event(&harness, LIFECYCLE_IO_STOP);
    apply_event(&harness, LIFECYCLE_D0_EXIT);
    apply_event(&harness, LIFECYCLE_RELEASE);
    assert(harness.requests_started == 1);
    assert(harness.requests_completed == 1);
    assert(harness.requests_cancelled == 0);
}

static void test_sleep_requires_new_owner_session(void) {
    activate_harness(&harness, true);
    apply_event(&harness, LIFECYCLE_D0_EXIT);
    assert(harness.file_accepted);
    assert(harness.session.state == XSNET_SESSION_CLOSED);
    harness.in_d0 = true;
    assert_lifecycle_invariants(&harness);
    apply_event(&harness, LIFECYCLE_CLEANUP);
    open_owner(&harness);
    assert(harness.session.state == XSNET_SESSION_OPENED);
    assert(harness.requests_cancelled == 1);
}

static void test_queue_restart_preserves_owner(void) {
    activate_harness(&harness, false);
    apply_event(&harness, LIFECYCLE_TX_CANCEL);
    assert(harness.file_accepted);
    assert(harness.session.state == XSNET_SESSION_ATTACHED);
    assert(harness.transmit_packets.count == 0);
    assert(harness.receive_packets.count == 1);
    harness.transmit_started = true;
    harness.session.state = XSNET_SESSION_LINK_UP;
    harness.link_connected = true;
    assert_lifecycle_invariants(&harness);
    apply_event(&harness, LIFECYCLE_CLEANUP);
    assert(harness.session.state == XSNET_SESSION_CLOSED);
}

static void test_repeated_teardown_is_idempotent(void) {
    uint32_t repeat;

    activate_harness(&harness, true);
    for (repeat = 0; repeat < 3; ++repeat) {
        apply_event(&harness, LIFECYCLE_IO_STOP);
        apply_event(&harness, LIFECYCLE_CLEANUP);
        apply_event(&harness, LIFECYCLE_TX_CANCEL);
        apply_event(&harness, LIFECYCLE_RX_CANCEL);
        apply_event(&harness, LIFECYCLE_D0_EXIT);
        apply_event(&harness, LIFECYCLE_RELEASE);
    }
    assert(harness.requests_started == 1);
    assert(harness.requests_completed == 0);
    assert(harness.requests_cancelled == 1);
}

int main(void) {
    test_all_teardown_interleavings();
    test_completion_wins_before_teardown();
    test_sleep_requires_new_owner_session();
    test_queue_restart_preserves_owner();
    test_repeated_teardown_is_idempotent();
    puts("xsnet lifecycle interleaving tests passed");
    return 0;
}
