#include "xsnet_session.h"

static uint16_t read_u16(const uint8_t *bytes) {
    return (uint16_t)((uint16_t)bytes[0] | ((uint16_t)bytes[1] << 8));
}

static uint32_t read_u32(const uint8_t *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
           ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static void reset_session(XsnetSession *session) {
    session->owner_cookie = 0;
    session->last_sequence = 0;
    session->negotiated_capabilities = 0;
    session->mtu = 0;
    session->transmit_queue_depth = 0;
    session->receive_queue_depth = 0;
    session->state = XSNET_SESSION_CLOSED;
}

static XsnetSessionStatus validate_sequence(
    const XsnetSession *session,
    const XsnetMessageView *message_view) {
    if (message_view->sequence == UINT64_MAX ||
        message_view->sequence <= session->last_sequence) {
        return XSNET_SESSION_BAD_SEQUENCE;
    }
    return XSNET_SESSION_OK;
}

static XsnetSessionStatus process_hello(
    XsnetSession *session,
    const XsnetMessageView *message_view) {
    uint16_t minimum_version;
    uint16_t maximum_version;
    uint32_t requested_capabilities;

    if (session->state != XSNET_SESSION_OPENED) {
        return XSNET_SESSION_BAD_STATE;
    }
    minimum_version = read_u16(message_view->payload);
    maximum_version = read_u16(message_view->payload + 2);
    requested_capabilities = read_u32(message_view->payload + 4);
    if (read_u32(message_view->payload + 8) != 0 ||
        read_u32(message_view->payload + 12) != 0) {
        return XSNET_SESSION_BAD_MESSAGE;
    }
    if (minimum_version > maximum_version ||
        minimum_version > XSNET_ABI_VERSION ||
        maximum_version < XSNET_ABI_VERSION) {
        return XSNET_SESSION_BAD_VERSION_RANGE;
    }
    if ((requested_capabilities & XSNET_CAPABILITY_IPV4) == 0 ||
        (requested_capabilities & ~XSNET_SESSION_DRIVER_CAPABILITIES) != 0) {
        return XSNET_SESSION_BAD_CAPABILITIES;
    }
    session->negotiated_capabilities =
        requested_capabilities & XSNET_SESSION_DRIVER_CAPABILITIES;
    session->state = XSNET_SESSION_NEGOTIATED;
    return XSNET_SESSION_OK;
}

static XsnetSessionStatus process_attach(
    XsnetSession *session,
    const XsnetMessageView *message_view) {
    uint32_t mtu;
    uint16_t transmit_queue_depth;
    uint16_t receive_queue_depth;
    uint32_t capabilities;

    if (session->state != XSNET_SESSION_NEGOTIATED) {
        return XSNET_SESSION_BAD_STATE;
    }
    mtu = read_u32(message_view->payload);
    transmit_queue_depth = read_u16(message_view->payload + 4);
    receive_queue_depth = read_u16(message_view->payload + 6);
    capabilities = read_u32(message_view->payload + 8);
    if (read_u32(message_view->payload + 12) != 0) {
        return XSNET_SESSION_BAD_MESSAGE;
    }
    if (mtu < XSNET_SESSION_MIN_MTU || mtu > XSNET_ABI_MAX_PACKET_SIZE) {
        return XSNET_SESSION_BAD_MTU;
    }
    if (transmit_queue_depth == 0 ||
        transmit_queue_depth > XSNET_SESSION_MAX_QUEUE_DEPTH ||
        receive_queue_depth == 0 ||
        receive_queue_depth > XSNET_SESSION_MAX_QUEUE_DEPTH) {
        return XSNET_SESSION_BAD_QUEUE_DEPTH;
    }
    if (capabilities != session->negotiated_capabilities) {
        return XSNET_SESSION_BAD_CAPABILITIES;
    }
    session->mtu = mtu;
    session->transmit_queue_depth = transmit_queue_depth;
    session->receive_queue_depth = receive_queue_depth;
    session->state = XSNET_SESSION_ATTACHED;
    return XSNET_SESSION_OK;
}

static XsnetSessionStatus process_set_link(
    XsnetSession *session,
    const XsnetMessageView *message_view) {
    uint32_t link_up;

    if (session->state != XSNET_SESSION_ATTACHED &&
        session->state != XSNET_SESSION_LINK_UP) {
        return XSNET_SESSION_BAD_STATE;
    }
    link_up = read_u32(message_view->payload);
    if (link_up > 1 || read_u32(message_view->payload + 4) != 0) {
        return XSNET_SESSION_BAD_MESSAGE;
    }
    session->state = link_up == 1 ? XSNET_SESSION_LINK_UP : XSNET_SESSION_ATTACHED;
    return XSNET_SESSION_OK;
}

static XsnetSessionStatus process_packet_batch(
    const XsnetSession *session,
    const XsnetMessageView *message_view,
    XsnetValidationStatus *validation_status) {
    uint16_t packet_count;

    if (session->state != XSNET_SESSION_LINK_UP) {
        return XSNET_SESSION_BAD_STATE;
    }
    *validation_status = XsnetValidatePacketBatchForMtu(
        message_view->payload,
        message_view->payload_length,
        session->mtu,
        &packet_count);
    if (*validation_status != XSNET_VALID) {
        return XSNET_SESSION_BAD_MESSAGE;
    }
    return XSNET_SESSION_OK;
}

static XsnetSessionStatus process_detach(XsnetSession *session) {
    if (session->state != XSNET_SESSION_NEGOTIATED &&
        session->state != XSNET_SESSION_ATTACHED &&
        session->state != XSNET_SESSION_LINK_UP) {
        return XSNET_SESSION_BAD_STATE;
    }
    session->mtu = 0;
    session->transmit_queue_depth = 0;
    session->receive_queue_depth = 0;
    session->state = XSNET_SESSION_NEGOTIATED;
    return XSNET_SESSION_OK;
}

void XsnetSessionInitialize(XsnetSession *session) {
    if (session != NULL) {
        reset_session(session);
    }
}

XsnetSessionStatus XsnetSessionOpen(
    XsnetSession *session,
    uint64_t owner_cookie) {
    if (session == NULL || owner_cookie == 0) {
        return XSNET_SESSION_INVALID_ARGUMENT;
    }
    if (session->state != XSNET_SESSION_CLOSED) {
        return XSNET_SESSION_BAD_STATE;
    }
    session->owner_cookie = owner_cookie;
    session->state = XSNET_SESSION_OPENED;
    return XSNET_SESSION_OK;
}

XsnetSessionStatus XsnetSessionClose(
    XsnetSession *session,
    uint64_t owner_cookie) {
    if (session == NULL || owner_cookie == 0) {
        return XSNET_SESSION_INVALID_ARGUMENT;
    }
    if (session->state == XSNET_SESSION_CLOSED ||
        session->owner_cookie != owner_cookie) {
        return XSNET_SESSION_BAD_OWNER;
    }
    reset_session(session);
    return XSNET_SESSION_OK;
}

XsnetSessionStatus XsnetSessionProcess(
    XsnetSession *session,
    uint64_t owner_cookie,
    XsnetMessageType message_type,
    const void *buffer,
    size_t buffer_length,
    XsnetValidationStatus *validation_status) {
    XsnetMessageView message_view;
    XsnetSessionStatus session_status;
    uint32_t maximum_payload_length;

    if (session == NULL || owner_cookie == 0 || validation_status == NULL) {
        return XSNET_SESSION_INVALID_ARGUMENT;
    }
    *validation_status = XSNET_INVALID_ARGUMENT;
    if (session->state == XSNET_SESSION_CLOSED ||
        session->owner_cookie != owner_cookie) {
        return XSNET_SESSION_BAD_OWNER;
    }

    switch (message_type) {
        case XSNET_MESSAGE_HELLO:
            maximum_payload_length = XSNET_ABI_HELLO_PAYLOAD_SIZE;
            break;
        case XSNET_MESSAGE_ATTACH:
            maximum_payload_length = XSNET_ABI_ATTACH_PAYLOAD_SIZE;
            break;
        case XSNET_MESSAGE_SET_LINK:
            maximum_payload_length = XSNET_ABI_SET_LINK_PAYLOAD_SIZE;
            break;
        case XSNET_MESSAGE_TX_BATCH:
        case XSNET_MESSAGE_RX_BATCH:
            maximum_payload_length = XSNET_ABI_MAX_PAYLOAD;
            break;
        case XSNET_MESSAGE_DETACH:
            maximum_payload_length = 0;
            break;
        default:
            return XSNET_SESSION_BAD_MESSAGE;
    }

    *validation_status = XsnetValidateMessage(
        buffer,
        buffer_length,
        message_type,
        maximum_payload_length,
        &message_view);
    if (*validation_status != XSNET_VALID) {
        return XSNET_SESSION_BAD_MESSAGE;
    }
    if (message_view.payload_length != maximum_payload_length &&
        message_type != XSNET_MESSAGE_TX_BATCH &&
        message_type != XSNET_MESSAGE_RX_BATCH) {
        return XSNET_SESSION_BAD_MESSAGE;
    }
    session_status = validate_sequence(session, &message_view);
    if (session_status != XSNET_SESSION_OK) {
        return session_status;
    }

    switch (message_type) {
        case XSNET_MESSAGE_HELLO:
            session_status = process_hello(session, &message_view);
            break;
        case XSNET_MESSAGE_ATTACH:
            session_status = process_attach(session, &message_view);
            break;
        case XSNET_MESSAGE_SET_LINK:
            session_status = process_set_link(session, &message_view);
            break;
        case XSNET_MESSAGE_TX_BATCH:
        case XSNET_MESSAGE_RX_BATCH:
            session_status = process_packet_batch(
                session,
                &message_view,
                validation_status);
            break;
        case XSNET_MESSAGE_DETACH:
            session_status = process_detach(session);
            break;
        default:
            session_status = XSNET_SESSION_BAD_MESSAGE;
            break;
    }
    if (session_status == XSNET_SESSION_OK) {
        session->last_sequence = message_view.sequence;
    }
    return session_status;
}
