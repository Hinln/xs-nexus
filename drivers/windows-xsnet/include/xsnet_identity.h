#ifndef XSNET_IDENTITY_H
#define XSNET_IDENTITY_H

#include <stddef.h>
#include <stdint.h>

#define XSNET_IDENTITY_VERSION UINT16_C(1)
#define XSNET_IDENTITY_REQUEST_SIZE UINT16_C(8)
#define XSNET_IDENTITY_RESPONSE_SIZE UINT16_C(16)

typedef enum XsnetIdentityStatus {
    XSNET_IDENTITY_VALID = 0,
    XSNET_IDENTITY_INVALID_ARGUMENT = 1,
    XSNET_IDENTITY_BAD_LENGTH = 2,
    XSNET_IDENTITY_BAD_VERSION = 3,
    XSNET_IDENTITY_BAD_RESERVED = 4,
    XSNET_IDENTITY_BAD_LUID = 5
} XsnetIdentityStatus;

XsnetIdentityStatus XsnetValidateIdentityRequest(
    const void *buffer,
    size_t buffer_length);

XsnetIdentityStatus XsnetWriteIdentityResponse(
    void *buffer,
    size_t buffer_length,
    uint64_t interface_luid);

#endif
