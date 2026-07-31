#ifndef XSNET_SESSION_H
#define XSNET_SESSION_H

#include "xsnet_abi.h"

#include <stddef.h>
#include <stdint.h>

#define XSNET_SESSION_MIN_MTU UINT32_C(1280)
#define XSNET_SESSION_MAX_QUEUE_DEPTH UINT16_C(64)
#define XSNET_SESSION_DRIVER_CAPABILITIES XSNET_CAPABILITY_IPV4

typedef enum XsnetSessionState {
    XSNET_SESSION_CLOSED = 0,
    XSNET_SESSION_OPENED = 1,
    XSNET_SESSION_NEGOTIATED = 2,
    XSNET_SESSION_ATTACHED = 3,
    XSNET_SESSION_LINK_UP = 4
} XsnetSessionState;

typedef enum XsnetSessionStatus {
    XSNET_SESSION_OK = 0,
    XSNET_SESSION_INVALID_ARGUMENT = 1,
    XSNET_SESSION_BAD_OWNER = 2,
    XSNET_SESSION_BAD_STATE = 3,
    XSNET_SESSION_BAD_MESSAGE = 4,
    XSNET_SESSION_BAD_SEQUENCE = 5,
    XSNET_SESSION_BAD_VERSION_RANGE = 6,
    XSNET_SESSION_BAD_CAPABILITIES = 7,
    XSNET_SESSION_BAD_MTU = 8,
    XSNET_SESSION_BAD_QUEUE_DEPTH = 9
} XsnetSessionStatus;

typedef struct XsnetSession {
    uint64_t owner_cookie;
    uint64_t last_sequence;
    uint32_t negotiated_capabilities;
    uint32_t mtu;
    uint16_t transmit_queue_depth;
    uint16_t receive_queue_depth;
    XsnetSessionState state;
} XsnetSession;

void XsnetSessionInitialize(XsnetSession *session);

XsnetSessionStatus XsnetSessionOpen(
    XsnetSession *session,
    uint64_t owner_cookie);

XsnetSessionStatus XsnetSessionClose(
    XsnetSession *session,
    uint64_t owner_cookie);

XsnetSessionStatus XsnetSessionProcess(
    XsnetSession *session,
    uint64_t owner_cookie,
    XsnetMessageType message_type,
    const void *buffer,
    size_t buffer_length,
    XsnetValidationStatus *validation_status);

#endif
