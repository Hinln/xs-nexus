#include "xsnet_identity.h"

#ifdef NDEBUG
#undef NDEBUG
#endif
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    uint8_t request[XSNET_IDENTITY_REQUEST_SIZE] = {1, 0, 8, 0, 0, 0, 0, 0};
    uint8_t response[XSNET_IDENTITY_RESPONSE_SIZE];
    const uint64_t luid = UINT64_C(0x1122334455667788);

    assert(XsnetValidateIdentityRequest(request, sizeof(request)) ==
           XSNET_IDENTITY_VALID);
    assert(XsnetValidateIdentityRequest(NULL, sizeof(request)) ==
           XSNET_IDENTITY_INVALID_ARGUMENT);
    assert(XsnetValidateIdentityRequest(request, sizeof(request) - 1) ==
           XSNET_IDENTITY_BAD_LENGTH);
    request[0] = 2;
    assert(XsnetValidateIdentityRequest(request, sizeof(request)) ==
           XSNET_IDENTITY_BAD_VERSION);
    request[0] = 1;
    request[4] = 1;
    assert(XsnetValidateIdentityRequest(request, sizeof(request)) ==
           XSNET_IDENTITY_BAD_RESERVED);

    memset(response, UINT8_C(0xa5), sizeof(response));
    assert(XsnetWriteIdentityResponse(response, sizeof(response), luid) ==
           XSNET_IDENTITY_VALID);
    assert(response[0] == 1 && response[1] == 0);
    assert(response[2] == 16 && response[3] == 0);
    assert(response[4] == 0 && response[5] == 0 &&
           response[6] == 0 && response[7] == 0);
    assert(memcmp(response + 8, "\x88\x77\x66\x55\x44\x33\x22\x11", 8) == 0);
    assert(XsnetWriteIdentityResponse(response, sizeof(response), 0) ==
           XSNET_IDENTITY_BAD_LUID);
    assert(XsnetWriteIdentityResponse(response, sizeof(response) - 1, luid) ==
           XSNET_IDENTITY_BAD_LENGTH);

    puts("xsnet identity tests passed");
    return 0;
}
