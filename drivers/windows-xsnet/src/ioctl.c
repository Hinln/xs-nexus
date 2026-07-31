#include "xsnet_driver.h"

static XsnetMessageType message_type_from_ioctl(ULONG io_control_code) {
    switch (io_control_code) {
        case IOCTL_XSNET_HELLO:
            return XSNET_MESSAGE_HELLO;
        case IOCTL_XSNET_ATTACH:
            return XSNET_MESSAGE_ATTACH;
        case IOCTL_XSNET_SET_LINK:
            return XSNET_MESSAGE_SET_LINK;
        case IOCTL_XSNET_DEQUEUE_TX:
            return XSNET_MESSAGE_TX_BATCH;
        case IOCTL_XSNET_ENQUEUE_RX:
            return XSNET_MESSAGE_RX_BATCH;
        case IOCTL_XSNET_DETACH:
            return XSNET_MESSAGE_DETACH;
        default:
            return 0;
    }
}

static NTSTATUS status_from_session(XsnetSessionStatus session_status) {
    switch (session_status) {
        case XSNET_SESSION_OK:
            return STATUS_SUCCESS;
        case XSNET_SESSION_BAD_OWNER:
            return STATUS_ACCESS_DENIED;
        case XSNET_SESSION_BAD_STATE:
            return STATUS_INVALID_DEVICE_STATE;
        case XSNET_SESSION_BAD_VERSION_RANGE:
            return STATUS_REVISION_MISMATCH;
        case XSNET_SESSION_BAD_CAPABILITIES:
            return STATUS_NOT_SUPPORTED;
        case XSNET_SESSION_INVALID_ARGUMENT:
        case XSNET_SESSION_BAD_MESSAGE:
        case XSNET_SESSION_BAD_SEQUENCE:
        case XSNET_SESSION_BAD_MTU:
        case XSNET_SESSION_BAD_QUEUE_DEPTH:
        default:
            return STATUS_INVALID_PARAMETER;
    }
}

static NTSTATUS validate_request_file(
    WDFREQUEST request,
    XsnetFileContext **file_context) {
    WDFFILEOBJECT file_object;

    if (WdfRequestGetRequestorMode(request) != UserMode) {
        return STATUS_ACCESS_DENIED;
    }
    file_object = WdfRequestGetFileObject(request);
    if (file_object == NULL) {
        return STATUS_ACCESS_DENIED;
    }
    *file_context = XsnetGetFileContext(file_object);
    if (!(*file_context)->accepted || (*file_context)->owner_cookie == 0) {
        return STATUS_ACCESS_DENIED;
    }
    return STATUS_SUCCESS;
}

void XsnetEvtIoDeviceControl(
    WDFQUEUE queue,
    WDFREQUEST request,
    size_t output_buffer_length,
    size_t input_buffer_length,
    ULONG io_control_code) {
    WDFDEVICE device = WdfIoQueueGetDevice(queue);
    XsnetDeviceContext *device_context = XsnetGetDeviceContext(device);
    XsnetFileContext *file_context;
    XsnetValidationStatus validation_status;
    XsnetSessionStatus session_status;
    XsnetMessageType message_type;
    PVOID input_buffer;
    size_t actual_input_length;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(output_buffer_length);

    status = validate_request_file(request, &file_context);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    message_type = message_type_from_ioctl(io_control_code);
    if (message_type == 0) {
        WdfRequestComplete(request, STATUS_INVALID_DEVICE_REQUEST);
        return;
    }
    if (message_type == XSNET_MESSAGE_SET_LINK ||
        message_type == XSNET_MESSAGE_TX_BATCH ||
        message_type == XSNET_MESSAGE_RX_BATCH) {
        WdfRequestComplete(request, STATUS_NOT_SUPPORTED);
        return;
    }
    if (input_buffer_length < XSNET_ABI_HEADER_SIZE ||
        input_buffer_length > XSNET_ABI_HEADER_SIZE + XSNET_ABI_MAX_PAYLOAD) {
        WdfRequestComplete(request, STATUS_INVALID_BUFFER_SIZE);
        return;
    }
    status = WdfRequestRetrieveInputBuffer(
        request,
        XSNET_ABI_HEADER_SIZE,
        &input_buffer,
        &actual_input_length);
    if (!NT_SUCCESS(status) || actual_input_length != input_buffer_length) {
        WdfRequestComplete(
            request,
            NT_SUCCESS(status) ? STATUS_INVALID_BUFFER_SIZE : status);
        return;
    }
    status = WdfWaitLockAcquire(device_context->session_lock, NULL);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    if (!file_context->accepted) {
        session_status = XSNET_SESSION_BAD_OWNER;
    } else {
        session_status = XsnetSessionProcess(
            &device_context->session,
            file_context->owner_cookie,
            message_type,
            input_buffer,
            actual_input_length,
            &validation_status);
    }
    if (session_status == XSNET_SESSION_OK &&
        message_type == XSNET_MESSAGE_ATTACH) {
        NetAdapterSetLinkLayerMtuSize(
            device_context->adapter,
            device_context->session.mtu);
    }
    WdfWaitLockRelease(device_context->session_lock);

    if (session_status == XSNET_SESSION_OK &&
        message_type == XSNET_MESSAGE_DETACH) {
        XsnetAdapterSetConnected(device, FALSE);
    }
    WdfRequestComplete(request, status_from_session(session_status));
}

void XsnetEvtIoStop(
    WDFQUEUE queue,
    WDFREQUEST request,
    ULONG action_flags) {
    UNREFERENCED_PARAMETER(queue);
    UNREFERENCED_PARAMETER(action_flags);
    WdfRequestComplete(request, STATUS_CANCELLED);
}
